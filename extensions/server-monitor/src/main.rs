//! The optional package computes metrics and owns histories; the host keeps SSH credentials.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Read, Write},
};

#[derive(Clone)]
struct Process {
    pid: u32,
    name: String,
    start: u64,
    ticks: u64,
    memory: u64,
}
struct Sample {
    total: u64,
    idle: u64,
    memory: u64,
    available: u64,
    disk_total: u64,
    disk_used: u64,
    uptime: f64,
    network: BTreeMap<String, (u64, u64)>,
    processes: BTreeMap<u32, Process>,
}
fn num(s: &str) -> Option<u64> {
    s.parse().ok()
}
fn process(line: &str, pagesize: u64) -> Option<Process> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close <= open {
        return None;
    }
    let pid: u32 = line[..open].trim().parse().ok()?;
    if pid == 0 {
        return None;
    }
    let fields: Vec<_> = line[close + 1..].split_whitespace().collect();
    let rss: i64 = fields.get(21)?.parse().ok()?;
    Some(Process {
        pid,
        name: line[open + 1..close]
            .chars()
            .filter(|c| !c.is_control())
            .take(128)
            .collect(),
        start: num(fields.get(19)?)?,
        ticks: num(fields.get(11)?)?.saturating_add(num(fields.get(12)?)?),
        memory: (rss.max(0) as u64).saturating_mul(pagesize),
    })
}
fn parse(raw: &str) -> Result<Sample, String> {
    let mut sections: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut section = "";
    for line in raw.lines() {
        if [
            xpeed_i18n::t("CPU"),
            "MEMORY",
            "UPTIME",
            "NETWORK",
            "DISK",
            "PAGESIZE",
            "PROCESSES",
        ]
        .contains(&line)
        {
            section = line;
        } else {
            sections.entry(section).or_default().push(line);
        }
    }
    let get = |name| sections.get(name).map(Vec::as_slice).unwrap_or(&[]);
    let cpu = get(xpeed_i18n::t("CPU"))
        .iter()
        .find(|line| line.starts_with("cpu "))
        .ok_or(xpeed_i18n::t("No hay contadores CPU de Linux"))?;
    let values: Vec<_> = cpu
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(num)
        .collect::<Option<_>>()
        .ok_or(xpeed_i18n::t("Contadores CPU inválidos"))?;
    if values.len() < 4 {
        return Err(xpeed_i18n::t("Muestra CPU incompleta").into());
    }
    let total = values
        .iter()
        .fold(0u64, |sum, value| sum.saturating_add(*value));
    let idle = values[3].saturating_add(*values.get(4).unwrap_or(&0));
    let mut memory = BTreeMap::new();
    for line in get("MEMORY") {
        if let Some((name, value)) = line.split_once(':')
            && let Some(value) = value.split_whitespace().next().and_then(num)
        {
            memory.insert(name, value.saturating_mul(1024));
        }
    }
    let mem = *memory
        .get("MemTotal")
        .ok_or(xpeed_i18n::t("No hay MemTotal"))?;
    if mem == 0 {
        return Err(xpeed_i18n::t("MemTotal inválido").into());
    }
    let available = *memory.get("MemAvailable").unwrap_or(
        &["MemFree", "Buffers", "Cached"]
            .iter()
            .map(|key| memory.get(key).copied().unwrap_or(0))
            .sum::<u64>(),
    );
    let uptime: f64 = get("UPTIME")
        .first()
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse().ok())
        .ok_or(xpeed_i18n::t("No hay uptime"))?;
    if !uptime.is_finite() || uptime < 0. {
        return Err(xpeed_i18n::t("Uptime inválido").into());
    }
    let mut network = BTreeMap::new();
    for line in get("NETWORK") {
        if let Some((name, values)) = line.split_once(':') {
            let name = name.trim();
            let fields: Vec<_> = values.split_whitespace().collect();
            if name != "lo"
                && let (Some(rx), Some(tx)) = (
                    fields.first().and_then(|s| num(s)),
                    fields.get(8).and_then(|s| num(s)),
                )
            {
                network.insert(name.to_string(), (rx, tx));
            }
        }
    }
    let disk: Vec<_> = get("DISK")
        .iter()
        .find(|line| line.split_whitespace().last() == Some("/"))
        .map(|line| line.split_whitespace().collect())
        .unwrap_or_default();
    let disk_total = disk
        .get(1)
        .and_then(|s| num(s))
        .unwrap_or(0)
        .saturating_mul(1024);
    let disk_used = disk
        .get(2)
        .and_then(|s| num(s))
        .unwrap_or(0)
        .saturating_mul(1024);
    let pagesize = get("PAGESIZE")
        .first()
        .and_then(|s| num(s))
        .filter(|n| *n > 0 && *n <= 1048576)
        .ok_or(xpeed_i18n::t("Falta getconf PAGESIZE"))?;
    let processes = get("PROCESSES")
        .iter()
        .filter_map(|line| process(line, pagesize))
        .map(|p| (p.pid, p))
        .take(32768)
        .collect();
    Ok(Sample {
        total,
        idle,
        memory: mem,
        available: available.min(mem),
        disk_total,
        disk_used,
        uptime,
        network,
        processes,
    })
}
fn bytes(value: f64) -> String {
    if value >= 1073741824. {
        format!("{:.1} GiB", value / 1073741824.)
    } else if value >= 1048576. {
        format!("{:.1} MiB", value / 1048576.)
    } else if value >= 1024. {
        format!("{:.1} KiB", value / 1024.)
    } else {
        format!("{value:.0} B")
    }
}
#[derive(Default)]
struct Monitor {
    context: Option<u64>,
    caption: String,
    observing: bool,
    previous: Option<Sample>,
    histories: BTreeMap<String, Vec<f64>>,
    dashboard: Value,
}
impl Monitor {
    fn context(&mut self, value: &Value) {
        self.context = value["id"].as_u64();
        self.observing = value["observing"].as_bool().unwrap_or(false);
        self.caption = if self.context.is_some() {
            format!(
                "{} · {}:{}",
                value["name"].as_str().unwrap_or(xpeed_i18n::t("Servidor")),
                value["host"].as_str().unwrap_or(""),
                value["port"].as_u64().unwrap_or(22)
            )
        } else {
            xpeed_i18n::t("Monitor del servidor").into()
        };
        self.previous = None;
        self.histories.clear();
        self.status(if self.context.is_none() {
            xpeed_i18n::t("Selecciona una pestaña SSH autenticada.")
        } else if !self.observing {
            xpeed_i18n::t("Monitor pausado mientras la vista está oculta.")
        } else {
            xpeed_i18n::t("Conectando al servidor…")
        });
    }
    fn status(&mut self, status: &str) {
        self.dashboard = json!({"contextId":self.context,"caption":self.caption,"status":status,"metrics":[],"processes":[]});
    }
    fn metric(
        &mut self,
        label: String,
        display: String,
        value: Option<f64>,
        max: Option<f64>,
    ) -> Value {
        let history_key = xpeed_i18n::t_in(xpeed_i18n::Language::English, &label).to_owned();
        let history = self.histories.entry(history_key).or_default();
        if let Some(value) = value
            && value.is_finite()
            && value >= 0.
        {
            history.push(value);
            if history.len() > 120 {
                history.remove(0);
            }
        }
        let max = max.unwrap_or_else(|| history.iter().copied().fold(1., f64::max) * 1.15);
        json!({"label":label,"display":display,"max":max,"history":history})
    }
    fn sample(&mut self, value: &Value) {
        if value["contextId"].as_u64() != self.context || !self.observing {
            return;
        }
        if let Some(error) = value["error"].as_str() {
            self.previous = None;
            self.status(&xpeed_i18n::format(
                "Sin actualizar: {error}",
                &[("error", error.to_string())],
            ));
            return;
        }
        let Some(raw) = value["sample"].as_str() else {
            return;
        };
        let sample = match parse(raw) {
            Ok(s) => s,
            Err(error) => {
                self.previous = None;
                self.status(&error);
                return;
            }
        };
        if self
            .previous
            .as_ref()
            .is_some_and(|p| sample.total < p.total || sample.uptime <= p.uptime)
        {
            self.previous = None;
            self.histories.clear();
        }
        let previous = self.previous.take();
        let delta = previous
            .as_ref()
            .and_then(|p| sample.total.checked_sub(p.total))
            .filter(|n| *n > 0);
        let cpu = delta.map(|total| {
            let idle = sample
                .idle
                .saturating_sub(previous.as_ref().unwrap().idle)
                .min(total);
            100. * (total - idle) as f64 / total as f64
        });
        let ram = sample.memory - sample.available;
        let mut metrics = vec![
            self.metric(
                xpeed_i18n::t("CPU").into(),
                cpu.map_or_else(
                    || xpeed_i18n::t("Calculando…").into(),
                    |n| format!("{n:.1}%"),
                ),
                cpu,
                Some(100.),
            ),
            self.metric(
                xpeed_i18n::t("RAM").into(),
                format!("{} / {}", bytes(ram as f64), bytes(sample.memory as f64)),
                Some(100. * ram as f64 / sample.memory as f64),
                Some(100.),
            ),
        ];
        if sample.disk_total > 0 {
            metrics.push(self.metric(
                xpeed_i18n::t("Disco /").into(),
                format!(
                    "{} / {}",
                    bytes(sample.disk_used as f64),
                    bytes(sample.disk_total as f64)
                ),
                Some(100. * sample.disk_used as f64 / sample.disk_total as f64),
                Some(100.),
            ));
        }
        let elapsed = previous
            .as_ref()
            .map(|p| sample.uptime - p.uptime)
            .filter(|n| *n > 0.);
        for (name, (rx, tx)) in sample.network.iter().take(4) {
            let rates = previous
                .as_ref()
                .and_then(|p| p.network.get(name))
                .zip(elapsed)
                .and_then(|((oldrx, oldtx), seconds)| {
                    Some((
                        rx.checked_sub(*oldrx)? as f64 / seconds,
                        tx.checked_sub(*oldtx)? as f64 / seconds,
                    ))
                });
            for (direction, rate) in [("RX", rates.map(|r| r.0)), ("TX", rates.map(|r| r.1))] {
                metrics.push(self.metric(
                    format!("{name} {direction}"),
                    rate.map_or_else(
                        || xpeed_i18n::t("Calculando…").into(),
                        |r| format!("{}/s", bytes(r)),
                    ),
                    rate,
                    None,
                ));
            }
        }
        let mut processes:Vec<_> = sample.processes.values().map(|p| {
            let ticks=previous.as_ref().and_then(|old|old.processes.get(&p.pid)).filter(|old|old.start==p.start).and_then(|old|p.ticks.checked_sub(old.ticks));
            let cpu=ticks.zip(delta).map_or(0.,|(ticks,total)|(100.*ticks as f64/total as f64).clamp(0.,100.));
            json!({"pid":p.pid,"name":p.name,"start":p.start,"cpu":cpu,"memory":100.*p.memory as f64/sample.memory as f64,"memoryBytes":p.memory})
        }).collect();
        processes.sort_by(|a, b| {
            b["cpu"]
                .as_f64()
                .unwrap_or(0.)
                .total_cmp(&a["cpu"].as_f64().unwrap_or(0.))
        });
        let mut best: BTreeMap<u64, Value> = processes
            .iter()
            .take(50)
            .map(|p| (p["pid"].as_u64().unwrap(), p.clone()))
            .collect();
        processes.sort_by(|a, b| b["memoryBytes"].as_u64().cmp(&a["memoryBytes"].as_u64()));
        for p in processes.into_iter().take(50) {
            best.insert(p["pid"].as_u64().unwrap(), p);
        }
        self.dashboard = json!({"contextId":self.context,"caption":self.caption,"status":if cpu.is_some() {xpeed_i18n::t("Actualizado · cada 3 s · CPU de procesos: % del servidor")} else {xpeed_i18n::t("Primera muestra · calculando tasas de CPU y red…")},"metrics":metrics,"processes":best.into_values().collect::<Vec<_>>()});
        self.previous = Some(sample);
    }
    fn view(&self) -> Value {
        json!({"viewId":"monitor","dashboard":self.dashboard})
    }
}
fn emit(value: Value) {
    let mut out = io::stdout().lock();
    let _ = serde_json::to_writer(&mut out, &value);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}
fn main() {
    let mut monitor = Monitor::default();
    monitor.status(xpeed_i18n::t("Selecciona una pestaña SSH autenticada."));
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    loop {
        let mut line = String::new();
        if (&mut reader)
            .take(2 * 1024 * 1024 + 1)
            .read_line(&mut line)
            .unwrap_or(0)
            == 0
        {
            break;
        }
        if line.len() > 2 * 1024 * 1024 || !line.ends_with('\n') {
            break;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            break;
        };
        let method = value["method"].as_str().unwrap_or("");
        let params = &value["params"];
        let result = match method {
            "initialize" => {
                xpeed_i18n::set_language(xpeed_i18n::Language::from_code(
                    params["language"].as_str().unwrap_or("en"),
                ));
                monitor.status(xpeed_i18n::t("Selecciona una pestaña SSH autenticada."));
                json!({"protocolVersion":1})
            }
            "locale/change" => {
                xpeed_i18n::set_language(xpeed_i18n::Language::from_code(
                    params["language"].as_str().unwrap_or("en"),
                ));
                if let Some(status) = monitor.dashboard["status"].as_str() {
                    monitor.dashboard["status"] = Value::String(xpeed_i18n::t(status).to_owned());
                }
                if let Some(metrics) = monitor.dashboard["metrics"].as_array_mut() {
                    for metric in metrics {
                        if let Some(label) = metric["label"].as_str() {
                            metric["label"] = Value::String(xpeed_i18n::t(label).to_owned());
                        }
                        if let Some(display) = metric["display"].as_str() {
                            metric["display"] = Value::String(xpeed_i18n::t(display).to_owned());
                        }
                    }
                }
                emit(json!({"jsonrpc":"2.0","method":"views/updated","params":monitor.view()}));
                Value::Null
            }
            "views/read" => monitor.view(),
            "server/context" => {
                monitor.context(params);
                emit(json!({"jsonrpc":"2.0","method":"views/updated","params":monitor.view()}));
                Value::Null
            }
            "server/sample" => {
                monitor.sample(params);
                emit(json!({"jsonrpc":"2.0","method":"views/updated","params":monitor.view()}));
                Value::Null
            }
            "shutdown" => break,
            _ => {
                if !value["id"].is_null() {
                    emit(
                        json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":-32601,"message":"Unknown method"}}),
                    );
                }
                continue;
            }
        };
        if !value["id"].is_null() {
            emit(json!({"jsonrpc":"2.0","id":value["id"],"result":result}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(second: bool, start: u64) -> String {
        let mut fields = vec!["0".to_string(); 22];
        fields[0] = "R".into();
        fields[11] = if second { "30" } else { "10" }.into();
        fields[19] = start.to_string();
        fields[21] = "100".into();
        let cpu = if second {
            "cpu 20 0 30 150 0 0 0 0 10 0"
        } else {
            "cpu 10 0 20 70 0 0 0 0 5 0"
        };
        let rx = if second { 500 } else { 100 };
        let tx = if second { 300 } else { 100 };
        format!(
            "CPU\n{cpu}\nMEMORY\nMemTotal: 1024 kB\nMemAvailable: 512 kB\nUPTIME\n{} 0\nNETWORK\n eth0: {rx} 0 0 0 0 0 0 0 {tx} 0 0 0 0 0 0 0\nDISK\nFilesystem 1024-blocks Used Available Capacity Mounted on\n/dev/vda 1024 512 512 50% /\nPAGESIZE\n4096\nPROCESSES\n123 (worker (test)) {}\n",
            if second { 4 } else { 2 },
            fields.join(" ")
        )
    }
    #[test]
    fn metrics_use_counter_deltas_available_memory_and_process_identity() {
        let first = parse(&fixture(false, 1)).unwrap();
        assert_eq!(first.total, 100); // guest is included in user ticks, never counted twice.
        assert_eq!(first.processes[&123].name, "worker (test)");
        let mut monitor = Monitor::default();
        monitor.context(&json!({"id":1,"name":"A","host":"a","observing":true}));
        monitor.sample(&json!({"contextId":1,"sample":fixture(false,1)}));
        monitor.sample(&json!({"contextId":1,"sample":fixture(true,1)}));
        assert_eq!(monitor.dashboard["metrics"][0]["history"][0], 20.);
        assert_eq!(monitor.dashboard["metrics"][1]["history"][0], 50.);
        assert_eq!(monitor.dashboard["metrics"][2]["history"][0], 50.);
        assert_eq!(monitor.dashboard["metrics"][3]["history"][0], 200.);
        assert_eq!(monitor.dashboard["processes"][0]["cpu"], 20.);
        assert_eq!(monitor.dashboard["processes"][0]["memoryBytes"], 409600);
    }
    #[test]
    fn switching_servers_drops_history_and_ignores_late_samples() {
        let mut monitor = Monitor::default();
        monitor.context(&json!({"id":1,"name":"A","host":"a","observing":true}));
        monitor.sample(&json!({"contextId":1,"sample":fixture(false,1)}));
        monitor.context(&json!({"id":2,"name":"B","host":"b","observing":true}));
        let before = monitor.dashboard.clone();
        monitor.sample(&json!({"contextId":1,"sample":fixture(true,1)}));
        assert_eq!(monitor.dashboard, before);
        assert!(monitor.histories.is_empty());
        assert!(monitor.previous.is_none());
        monitor.sample(&json!({"contextId":2,"sample":fixture(false,1)}));
        monitor.sample(&json!({"contextId":2,"sample":fixture(true,2)}));
        assert_eq!(monitor.dashboard["processes"][0]["cpu"], 0.); // recycled PID.
        monitor.context(&json!({"id":null,"observing":false}));
        assert!(monitor.dashboard["metrics"].as_array().unwrap().is_empty());
    }
    #[test]
    fn histories_are_bounded_and_bad_samples_do_not_produce_fake_zeroes() {
        let mut monitor = Monitor::default();
        monitor.context(&json!({"id":1,"observing":true}));
        for i in 0..200 {
            monitor.metric("CPU".into(), String::new(), Some(i as f64), Some(100.));
        }
        assert_eq!(monitor.histories["CPU"].len(), 120);
        monitor.sample(&json!({"contextId":1,"sample":"CPU\nnot Linux\n"}));
        assert!(monitor.dashboard["metrics"].as_array().unwrap().is_empty());
        assert!(
            monitor.dashboard["status"]
                .as_str()
                .unwrap()
                .contains("CPU")
        );
    }
}
