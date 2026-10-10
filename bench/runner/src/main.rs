//! Le benchmark de Vitesse contre actix-web, axum et Drogon (C++), avec
//! [wrk](https://github.com/wg/wrk).
//!
//! ```sh
//! cargo run --release --manifest-path bench/runner/Cargo.toml -- [durée] [connexions]
//! ```
//!
//! Le serveur et wrk sont épinglés sur des cœurs distincts (`SERVER_CPUS`,
//! `CLIENT_CPUS`, par défaut `0,1` et `2,3`). En plus du débit, on mesure le
//! temps CPU consommé par le serveur pour chaque requête (lu dans `/proc`) :
//! une mesure qui ne dépend pas de wrk.
//!
//! Prérequis : Linux, `wrk` et `taskset`, et Drogon installé pour
//! `bench/drogon` (sinon il est ignoré ; `DROGON_PREFIX=/chemin/install` si
//! besoin). `WRK=/chemin/wrk` pour un autre binaire.

use std::collections::BTreeMap;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

/// (nom, chemin, script wrk, argument du script)
const SCENARIOS: [(&str, &str, Option<&str>, Option<&str>); 6] = [
    ("texte", "/", None, None),
    ("json", "/json", None, None),
    (
        "navigateur",
        "/json",
        Some("bench/lua/navigateur.lua"),
        None,
    ),
    ("params", "/users/42", None, None),
    ("post-json", "/echo", Some("bench/lua/post.lua"), None),
    (
        "pipeline×16",
        "/",
        Some("bench/lua/pipeline.lua"),
        Some("16"),
    ),
];

struct Config {
    duration: String,
    connections: String,
    server_cpus: String,
    client_cpus: String,
    workers: usize,
    wrk: String,
}

/// Un serveur à mesurer.
struct Server {
    name: &'static str,
    port: u16,
    program: PathBuf,
}

/// Ce que wrk a mesuré pour un scénario.
struct Measure {
    requests: u64,
    rps: f64,
    cpu_us: f64,
    errors: Option<u64>,
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::env::set_current_dir(&root).expect("dossier du dépôt");
    let mut args = std::env::args().skip(1);
    let server_cpus = env_or("SERVER_CPUS", "0,1");
    let config = Config {
        duration: args.next().unwrap_or_else(|| "10s".into()),
        connections: args.next().unwrap_or_else(|| "128".into()),
        workers: cpu_count(&server_cpus),
        server_cpus,
        client_cpus: env_or("CLIENT_CPUS", "2,3"),
        wrk: env_or("WRK", "wrk"),
    };
    if Command::new(&config.wrk).arg("--version").output().is_err() {
        fail("wrk introuvable (apt install wrk / brew install wrk), ou WRK=/chemin/wrk");
    }

    build(
        &["cargo", "build", "--release", "--example", "bench", "-q"],
        ".",
    );
    build(&["cargo", "build", "--release", "-q"], "bench/axum");
    build(&["cargo", "build", "--release", "-q"], "bench/actix");
    let drogon = build_drogon();

    let mut servers = Vec::new();
    if let Some(program) = drogon {
        servers.push(Server {
            name: "Drogon",
            port: 3003,
            program,
        });
    }
    servers.extend([
        Server {
            name: "axum",
            port: 3002,
            program: "bench/axum/target/release/bench-axum".into(),
        },
        Server {
            name: "actix-web",
            port: 3004,
            program: "bench/actix/target/release/bench-actix".into(),
        },
        Server {
            name: "Vitesse",
            port: 3000,
            program: "target/release/examples/bench".into(),
        },
    ]);

    println!(
        "Serveur sur les CPU {}, wrk sur {}, {} connexions, {} par scénario",
        config.server_cpus, config.client_cpus, config.connections, config.duration
    );
    let mut results: BTreeMap<(&str, &str), Measure> = BTreeMap::new();
    for server in &servers {
        for (scenario, measure) in run_server(server, &config) {
            results.insert((scenario, server.name), measure);
        }
    }
    print_table(&servers, &results);
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

/// Le nombre de cœurs d'une liste `taskset` : `0,1`, `0-3`, `0-1,4`…
fn cpu_count(list: &str) -> usize {
    list.split(',')
        .filter(|part| !part.trim().is_empty())
        .map(|part| match part.trim().split_once('-') {
            Some((a, b)) => match (a.parse::<usize>(), b.parse::<usize>()) {
                (Ok(a), Ok(b)) if b >= a => b - a + 1,
                _ => 1,
            },
            None => 1,
        })
        .sum::<usize>()
        .max(1)
}

fn build(command: &[&str], dir: &str) {
    let status = Command::new(command[0])
        .args(&command[1..])
        .current_dir(dir)
        .status()
        .unwrap_or_else(|e| fail(&format!("{}: {e}", command.join(" "))));
    if !status.success() {
        fail(&format!("échec de `{}` dans {dir}", command.join(" ")));
    }
}

/// Compile le serveur Drogon s'il ne l'est pas déjà ; `None` si Drogon
/// n'est pas installé.
fn build_drogon() -> Option<PathBuf> {
    let program = PathBuf::from("bench/drogon/build/bench-drogon");
    if program.exists() {
        return Some(program);
    }
    let mut configure = Command::new("cmake");
    configure.args([
        "-S",
        "bench/drogon",
        "-B",
        "bench/drogon/build",
        "-DCMAKE_BUILD_TYPE=Release",
    ]);
    if let Ok(prefix) = std::env::var("DROGON_PREFIX") {
        configure.arg(format!("-DCMAKE_PREFIX_PATH={prefix}"));
    }
    let quiet = |c: &mut Command| {
        c.stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    let built = quiet(&mut configure)
        && quiet(Command::new("cmake").args(["--build", "bench/drogon/build", "-j"]));
    if built && program.exists() {
        Some(program)
    } else {
        eprintln!("Drogon introuvable : bench/drogon ignoré");
        None
    }
}

fn wait_port(port: u16) {
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        sleep(Duration::from_millis(100));
    }
    fail(&format!("le serveur ne répond pas sur le port {port}"));
}

/// Temps CPU (utilisateur + système, en tops d'horloge) d'un processus et de
/// tous ses threads.
fn cpu_ticks(pid: u32) -> u64 {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return 0;
    };
    // Le nom du programme, entre parenthèses, peut contenir des espaces.
    let fields: Vec<&str> = stat
        .rsplit_once(')')
        .map_or("", |(_, rest)| rest)
        .split_whitespace()
        .collect();
    // Champs 14 et 15 de /proc/PID/stat (utime, stime), comptés après le nom.
    let field = |i: usize| {
        fields
            .get(i)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    field(11) + field(12)
}

/// Tops d'horloge par seconde (`getconf CLK_TCK`, 100 presque partout).
fn clock_ticks() -> f64 {
    Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(100.0)
}

fn load(
    config: &Config,
    port: u16,
    path: &str,
    script: Option<&str>,
    arg: Option<&str>,
    duration: &str,
) -> String {
    let mut command = Command::new("taskset");
    command
        .args(["-c", &config.client_cpus, &config.wrk])
        .arg(format!("-t{}", config.workers))
        .arg(format!("-c{}", config.connections))
        .arg(format!("-d{duration}"))
        .arg("--latency");
    if let Some(script) = script {
        command.args(["-s", script]);
    }
    command.arg(format!("http://127.0.0.1:{port}{path}"));
    if let Some(arg) = arg {
        command.args(["--", arg]);
    }
    let output = command
        .output()
        .unwrap_or_else(|e| fail(&format!("wrk : {e}")));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn number_after<T: std::str::FromStr>(text: &str, label: &str) -> Option<T> {
    let rest = &text[text.find(label)? + label.len()..];
    rest.split_whitespace().next()?.parse().ok()
}

fn parse_wrk(text: &str) -> Option<(u64, f64, Option<u64>)> {
    // « 2903410 requests in 10.00s, … »
    let requests = text
        .lines()
        .find(|l| l.contains(" requests in "))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let rps = number_after(text, "Requests/sec:")?;
    let errors = number_after(text, "Non-2xx or 3xx responses:");
    Some((requests, rps, errors))
}

fn stop(mut child: Child) {
    let _ = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status();
    let _ = child.wait();
    sleep(Duration::from_millis(500));
}

fn run_server(server: &Server, config: &Config) -> Vec<(&'static str, Measure)> {
    let child = Command::new("taskset")
        .args(["-c", &config.server_cpus])
        .arg(&server.program)
        .env("WORKERS", config.workers.to_string())
        .env("PORT", server.port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| fail(&format!("{} : {e}", server.program.display())));
    let pid = child.id();
    wait_port(server.port);
    // Échauffement (caches, allocateurs…).
    load(config, server.port, "/", None, None, "2s");
    let ticks_per_second = clock_ticks();
    let mut out = Vec::new();
    for (name, path, script, arg) in SCENARIOS {
        let before = cpu_ticks(pid);
        let text = load(config, server.port, path, script, arg, &config.duration);
        let after = cpu_ticks(pid);
        let Some((requests, rps, errors)) = parse_wrk(&text) else {
            eprintln!("{} / {name} : sortie de wrk illisible\n{text}", server.name);
            continue;
        };
        let cpu_us = (after - before) as f64 / ticks_per_second * 1e6 / requests.max(1) as f64;
        out.push((
            name,
            Measure {
                requests,
                rps,
                cpu_us,
                errors,
            },
        ));
    }
    stop(child);
    out
}

/// `2781541` → `2 781 541`.
fn grouped(n: f64) -> String {
    let digits = format!("{n:.0}");
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

fn print_table(servers: &[Server], results: &BTreeMap<(&str, &str), Measure>) {
    println!();
    println!("Requêtes par seconde (temps CPU serveur par requête) :");
    println!();
    let names: Vec<&str> = servers.iter().map(|s| s.name).collect();
    println!("| Scénario | {} |", names.join(" | "));
    println!("|---|{}", "---:|".repeat(names.len()));
    for (scenario, ..) in SCENARIOS {
        let cells: Vec<String> = names
            .iter()
            .map(|name| match results.get(&(scenario, *name)) {
                None => "—".to_owned(),
                Some(m) => {
                    let mut cell = format!("{} ({:.2} µs)", grouped(m.rps), m.cpu_us);
                    if *name == "Vitesse" {
                        cell = format!("**{cell}**");
                    }
                    if let Some(errors) = m.errors.filter(|&e| e > 0) {
                        cell.push_str(&format!(" ⚠ {errors} erreurs sur {}", m.requests));
                    }
                    cell
                }
            })
            .collect();
        println!("| {scenario} | {} |", cells.join(" | "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_cpu_lists() {
        assert_eq!(cpu_count("0,1"), 2);
        assert_eq!(cpu_count("0-3"), 4);
        assert_eq!(cpu_count("0-1,4"), 3);
        assert_eq!(cpu_count("2"), 1);
        assert_eq!(cpu_count(""), 1);
    }

    #[test]
    fn parses_wrk_output() {
        let text = "Running 10s test @ http://127.0.0.1:3000/\n  2 threads and 128 connections\n  2903410 requests in 10.00s, 360.95MB read\n  Non-2xx or 3xx responses: 12\nRequests/sec: 290341.00\nTransfer/sec:     36.09MB\n";
        assert_eq!(parse_wrk(text), Some((2_903_410, 290_341.0, Some(12))));
        assert_eq!(parse_wrk("rien"), None);
    }

    #[test]
    fn groups_thousands() {
        assert_eq!(grouped(2_781_541.4), "2 781 541");
        assert_eq!(grouped(942.0), "942");
    }
}
