//! Unified Logging
//!
//! Aggregates logs from all components into a unified stream.

use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "TRACE"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Fatal => write!(f, "FATAL"),
        }
    }
}
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: u64,

    pub component_id: String,

    pub level: LogLevel,

    pub message: String,

    pub fields: HashMap<String, String>,
}

impl LogEntry {
    pub fn new(component_id: &str, level: LogLevel, message: &str) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            timestamp,
            component_id: component_id.to_string(),
            level,
            message: message.to_string(),
            fields: HashMap::new(),
        }
    }
    pub fn with_field(mut self, key: &str, value: &str) -> Self {
        self.fields.insert(key.to_string(), value.to_string());
        self
    }
    pub fn format(&self) -> String {
        let time = chrono_format(self.timestamp);
        format!(
            "[{}] {} [{}] {}",
            time, self.level, self.component_id, self.message
        )
    }
    pub fn format_json(&self) -> String {
        let mut json = format!(
            r#"{{"timestamp":{},"level":"{}","component":"{}","message":"{}""#,
            self.timestamp,
            self.level,
            self.component_id,
            escape_json(&self.message)
        );

        if !self.fields.is_empty() {
            json.push_str(r#","fields":{"#);
            let fields: Vec<String> = self
                .fields
                .iter()
                .map(|(k, v)| format!(r#""{}":"{}""#, k, escape_json(v)))
                .collect();
            json.push_str(&fields.join(","));
            json.push('}');
        }

        json.push('}');
        json
    }
}
pub struct LogAggregator {
    component_logs: RwLock<HashMap<String, VecDeque<LogEntry>>>,

    all_logs: RwLock<VecDeque<LogEntry>>,

    max_per_component: usize,

    max_total: usize,

    min_level: RwLock<LogLevel>,
}

impl LogAggregator {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            component_logs: RwLock::new(HashMap::new()),
            all_logs: RwLock::new(VecDeque::with_capacity(buffer_size)),
            max_per_component: buffer_size,
            max_total: buffer_size * 10,
            min_level: RwLock::new(LogLevel::Trace),
        }
    }
    pub fn set_min_level(&self, level: LogLevel) {
        let mut min_level = self.min_level.write().unwrap();
        *min_level = level;
    }
    pub fn log(&self, component_id: &str, level: LogLevel, message: &str) {
        let min_level = *self.min_level.read().unwrap();
        if level < min_level {
            return;
        }

        let entry = LogEntry::new(component_id, level, message);

        {
            let mut component_logs = self.component_logs.write().unwrap();
            let logs = component_logs
                .entry(component_id.to_string())
                .or_insert_with(|| VecDeque::with_capacity(self.max_per_component));

            if logs.len() >= self.max_per_component {
                logs.pop_front();
            }
            logs.push_back(entry.clone());
        }

        {
            let mut all_logs = self.all_logs.write().unwrap();
            if all_logs.len() >= self.max_total {
                all_logs.pop_front();
            }
            all_logs.push_back(entry);
        }
    }
    pub fn log_with_fields(
        &self,
        component_id: &str,
        level: LogLevel,
        message: &str,
        fields: HashMap<String, String>,
    ) {
        let min_level = *self.min_level.read().unwrap();
        if level < min_level {
            return;
        }

        let mut entry = LogEntry::new(component_id, level, message);
        entry.fields = fields;

        {
            let mut component_logs = self.component_logs.write().unwrap();
            let logs = component_logs
                .entry(component_id.to_string())
                .or_insert_with(|| VecDeque::with_capacity(self.max_per_component));

            if logs.len() >= self.max_per_component {
                logs.pop_front();
            }
            logs.push_back(entry.clone());
        }

        {
            let mut all_logs = self.all_logs.write().unwrap();
            if all_logs.len() >= self.max_total {
                all_logs.pop_front();
            }
            all_logs.push_back(entry);
        }
    }
    pub fn get_logs(&self, component_id: &str, limit: usize) -> Vec<LogEntry> {
        let component_logs = self.component_logs.read().unwrap();
        component_logs
            .get(component_id)
            .map(|logs| {
                logs.iter()
                    .rev()
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn get_logs_by_level(
        &self,
        component_id: &str,
        min_level: LogLevel,
        limit: usize,
    ) -> Vec<LogEntry> {
        let component_logs = self.component_logs.read().unwrap();
        component_logs
            .get(component_id)
            .map(|logs| {
                logs.iter()
                    .filter(|e| e.level >= min_level)
                    .rev()
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn get_all_logs(&self, limit: usize) -> Vec<LogEntry> {
        let all_logs = self.all_logs.read().unwrap();
        all_logs
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
    pub fn get_all_logs_by_level(&self, min_level: LogLevel, limit: usize) -> Vec<LogEntry> {
        let all_logs = self.all_logs.read().unwrap();
        all_logs
            .iter()
            .filter(|e| e.level >= min_level)
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
    pub fn clear_component(&self, component_id: &str) {
        let mut component_logs = self.component_logs.write().unwrap();
        component_logs.remove(component_id);
    }
    pub fn clear_all(&self) {
        let mut component_logs = self.component_logs.write().unwrap();
        component_logs.clear();

        let mut all_logs = self.all_logs.write().unwrap();
        all_logs.clear();
    }
    pub fn stats(&self) -> LogStats {
        let component_logs = self.component_logs.read().unwrap();
        let all_logs = self.all_logs.read().unwrap();

        let mut by_level = [0usize; 6];
        for entry in all_logs.iter() {
            by_level[entry.level as usize] += 1;
        }

        LogStats {
            total_entries: all_logs.len(),
            component_count: component_logs.len(),
            by_level,
        }
    }
}
#[derive(Debug, Clone)]
pub struct LogStats {
    pub total_entries: usize,

    pub component_count: usize,

    pub by_level: [usize; 6],
}
fn chrono_format(timestamp_ms: u64) -> String {
    let secs = timestamp_ms / 1000;
    let ms = timestamp_ms % 1000;
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let secs = secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", hours, mins, secs, ms)
}
fn escape_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry() {
        let entry = LogEntry::new("test-component", LogLevel::Info, "Hello, world!");
        assert_eq!(entry.component_id, "test-component");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.message, "Hello, world!");
    }

    #[test]
    fn test_log_aggregator() {
        let aggregator = LogAggregator::new(100);

        aggregator.log("comp1", LogLevel::Info, "Message 1");
        aggregator.log("comp1", LogLevel::Warn, "Message 2");
        aggregator.log("comp2", LogLevel::Error, "Message 3");

        let comp1_logs = aggregator.get_logs("comp1", 10);
        assert_eq!(comp1_logs.len(), 2);

        let all_logs = aggregator.get_all_logs(10);
        assert_eq!(all_logs.len(), 3);

        let stats = aggregator.stats();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.component_count, 2);
    }

    #[test]
    fn test_log_level_filter() {
        let aggregator = LogAggregator::new(100);
        aggregator.set_min_level(LogLevel::Warn);

        aggregator.log("comp1", LogLevel::Debug, "Debug message");
        aggregator.log("comp1", LogLevel::Warn, "Warning message");

        let logs = aggregator.get_all_logs(10);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, LogLevel::Warn);
    }
}
