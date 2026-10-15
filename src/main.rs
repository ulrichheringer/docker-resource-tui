use anyhow::{Context, Result};
use bollard::container::{ListContainersOptions, Stats, StatsOptions};
use bollard::models::ContainerSummary;
use bollard::Docker;
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use docker_resource_tui::{calculate_cpu_percent, sort_containers, ContainerRow, SortBy};
use futures_util::{future::join_all, StreamExt};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

type Tui = Terminal<CrosstermBackend<Stdout>>;

struct App {
    rows: Vec<ContainerRow>,
    selected: usize,
    sort_by: SortBy,
    ascending: bool,
    message: String,
}

impl App {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            selected: 0,
            sort_by: SortBy::Name,
            ascending: true,
            message: "Connecting to Docker…".into(),
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            self.selected = 0;
            return;
        }
        let last = self.rows.len() - 1;
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    fn sort(&mut self) {
        sort_containers(&mut self.rows, self.sort_by, self.ascending);
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let docker = Docker::connect_with_local_defaults()
        .context("could not connect to the local Docker Engine")?;
    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal, &docker).await;
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> Result<Tui> {
    enable_raw_mode().context("could not enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("could not enter alternate screen")?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode().context("could not disable terminal raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("could not leave alternate screen")?;
    terminal.show_cursor()?;
    Ok(())
}

async fn run_app(terminal: &mut Tui, docker: &Docker) -> Result<()> {
    let mut app = App::new();
    let mut last_refresh = Instant::now() - Duration::from_secs(3);
    loop {
        if last_refresh.elapsed() >= Duration::from_secs(2) {
            match fetch_containers(docker).await {
                Ok(rows) => {
                    app.rows = rows;
                    app.sort();
                    app.message = format!("{} container(s)", app.rows.len());
                }
                Err(error) => app.message = format!("Docker refresh failed: {error:#}"),
            }
            last_refresh = Instant::now();
        }
        terminal.draw(|frame| draw(frame, &app))?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
                    KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
                    KeyCode::Char('s') => {
                        app.sort_by = app.sort_by.next();
                        app.sort();
                    }
                    KeyCode::Char('o') => {
                        app.ascending = !app.ascending;
                        app.sort();
                    }
                    KeyCode::Char('r') => last_refresh = Instant::now() - Duration::from_secs(3),
                    _ => {}
                }
            }
        }
    }
}

async fn fetch_containers(docker: &Docker) -> Result<Vec<ContainerRow>> {
    let options = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    let containers = docker.list_containers(Some(options)).await?;
    let requests = containers
        .into_iter()
        .map(|container| fetch_row(docker, container));
    Ok(join_all(requests).await)
}

async fn fetch_row(docker: &Docker, container: ContainerSummary) -> ContainerRow {
    let id = container.id.unwrap_or_default();
    let name = container
        .names
        .and_then(|names| names.into_iter().next())
        .map(|name| name.trim_start_matches('/').to_owned())
        .unwrap_or_else(|| id.chars().take(12).collect());
    let mut row = ContainerRow {
        id: id.clone(),
        name,
        state: container.state.unwrap_or_else(|| "unknown".into()),
        ..Default::default()
    };

    if row.state == "running" {
        let options = StatsOptions {
            stream: false,
            one_shot: true,
        };
        if let Some(Ok(stats)) = docker.stats(&id, Some(options)).next().await {
            apply_stats(&mut row, stats);
        }
    }
    row
}

fn apply_stats(row: &mut ContainerRow, stats: Stats) {
    row.cpu_percent = calculate_cpu_percent(
        stats.cpu_stats.cpu_usage.total_usage,
        stats.precpu_stats.cpu_usage.total_usage,
        stats.cpu_stats.system_cpu_usage.unwrap_or_default(),
        stats.precpu_stats.system_cpu_usage.unwrap_or_default(),
        stats.cpu_stats.online_cpus.unwrap_or(1) as u32,
    );
    row.memory_used = stats.memory_stats.usage;
    row.memory_limit = stats.memory_stats.limit;
    if let Some(networks) = stats.networks {
        row.network_rx = Some(networks.values().map(|net| net.rx_bytes).sum());
        row.network_tx = Some(networks.values().map(|net| net.tx_bytes).sum());
    }
}

fn draw(frame: &mut ratatui::Frame<'_>, app: &App) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(frame.area());
    let rows = app.rows.iter().map(|container| {
        Row::new(vec![
            Cell::from(container.name.as_str()),
            Cell::from(container.id.chars().take(12).collect::<String>()),
            Cell::from(container.state.as_str()),
            Cell::from(
                container
                    .cpu_percent
                    .map(|value| format!("{value:.1}%"))
                    .unwrap_or_else(|| "—".into()),
            ),
            Cell::from(format_bytes(container.memory_used)),
            Cell::from(format_bytes(container.memory_limit)),
            Cell::from(format_bytes(container.network_rx)),
            Cell::from(format_bytes(container.network_tx)),
        ])
    });
    let header = Row::new([
        "NAME",
        "ID",
        "STATE",
        "CPU",
        "MEM USED",
        "MEM LIMIT",
        "NET RX",
        "NET TX",
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let table = Table::new(
        rows,
        [
            Constraint::Min(16),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(9),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(11),
            Constraint::Length(11),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Docker resources "),
    )
    .row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▶ ");
    let mut state = TableState::default();
    if !app.rows.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(table, chunks[0], &mut state);

    let direction = if app.ascending {
        "ascending"
    } else {
        "descending"
    };
    let footer = Paragraph::new(format!(
        "{} | Sort: {} ({direction}) | ↑/↓ or j/k: select  s: sort  o: order  r: refresh  q: quit",
        app.message,
        app.sort_by.label(),
    ));
    frame.render_widget(footer, chunks[1]);
}

fn format_bytes(value: Option<u64>) -> String {
    let Some(value) = value else {
        return "—".into();
    };
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut amount = value as f64;
    let mut unit = 0;
    while amount >= 1024.0 && unit < UNITS.len() - 1 {
        amount /= 1024.0;
        unit += 1;
    }
    format!("{amount:.1} {}", UNITS[unit])
}
