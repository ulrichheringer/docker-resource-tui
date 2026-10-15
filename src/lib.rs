use std::cmp::Ordering;

#[derive(Clone, Debug, Default)]
pub struct ContainerRow {
    pub id: String,
    pub name: String,
    pub state: String,
    pub cpu_percent: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_limit: Option<u64>,
    pub network_rx: Option<u64>,
    pub network_tx: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Cpu,
    Memory,
    Network,
}

pub fn calculate_cpu_percent(
    current_cpu: u64,
    previous_cpu: u64,
    current_system: u64,
    previous_system: u64,
    online_cpus: u32,
) -> Option<f64> {
    let system_delta = current_system.saturating_sub(previous_system);
    if system_delta == 0 {
        return None;
    }
    let cpu_delta = current_cpu.saturating_sub(previous_cpu);
    Some(cpu_delta as f64 / system_delta as f64 * online_cpus.max(1) as f64 * 100.0)
}

impl SortBy {
    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::Cpu,
            Self::Cpu => Self::Memory,
            Self::Memory => Self::Network,
            Self::Network => Self::Name,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Cpu => "CPU",
            Self::Memory => "memory",
            Self::Network => "network RX",
        }
    }
}

pub fn sort_containers(rows: &mut [ContainerRow], sort_by: SortBy, ascending: bool) {
    rows.sort_by(|left, right| match sort_by {
        SortBy::Name => {
            let order = left.name.to_lowercase().cmp(&right.name.to_lowercase());
            if ascending {
                order
            } else {
                order.reverse()
            }
        }
        SortBy::Cpu => compare_optional_f64(left.cpu_percent, right.cpu_percent, ascending),
        SortBy::Memory => compare_optional(left.memory_used, right.memory_used, ascending),
        SortBy::Network => compare_optional(left.network_rx, right.network_rx, ascending),
    });
}

fn compare_optional<T: Ord>(left: Option<T>, right: Option<T>, ascending: bool) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => {
            let order = left.cmp(&right);
            if ascending {
                order
            } else {
                order.reverse()
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_optional_f64(left: Option<f64>, right: Option<f64>, ascending: bool) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => {
            let order = left.total_cmp(&right);
            if ascending {
                order
            } else {
                order.reverse()
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::{calculate_cpu_percent, sort_containers, ContainerRow, SortBy};

    #[test]
    fn calculates_cpu_percentage_from_docker_deltas() {
        assert_eq!(calculate_cpu_percent(150, 50, 400, 200, 2), Some(100.0));
        assert_eq!(calculate_cpu_percent(10, 5, 20, 20, 1), None);
    }

    #[test]
    fn sorts_by_name_and_cpu() {
        let mut rows = vec![
            ContainerRow {
                name: "zebra".into(),
                cpu_percent: Some(3.0),
                ..Default::default()
            },
            ContainerRow {
                name: "alpha".into(),
                cpu_percent: Some(7.0),
                ..Default::default()
            },
        ];
        sort_containers(&mut rows, SortBy::Name, true);
        assert_eq!(rows[0].name, "alpha");
        sort_containers(&mut rows, SortBy::Cpu, false);
        assert_eq!(rows[0].cpu_percent, Some(7.0));
    }

    #[test]
    fn keeps_missing_metrics_after_known_metrics_when_ascending() {
        let mut rows = vec![
            ContainerRow {
                name: "unknown".into(),
                memory_used: None,
                ..Default::default()
            },
            ContainerRow {
                name: "known".into(),
                memory_used: Some(512),
                ..Default::default()
            },
        ];
        sort_containers(&mut rows, SortBy::Memory, true);
        assert_eq!(rows[0].name, "known");
    }

    #[test]
    fn keeps_missing_metrics_after_known_metrics_when_descending() {
        let mut rows = vec![
            ContainerRow {
                name: "unknown".into(),
                memory_used: None,
                ..Default::default()
            },
            ContainerRow {
                name: "known".into(),
                memory_used: Some(512),
                ..Default::default()
            },
        ];
        sort_containers(&mut rows, SortBy::Memory, false);
        assert_eq!(rows[0].name, "known");
    }
}
