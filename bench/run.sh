#!/usr/bin/env bash
# Benchmark Vitesse vs Express avec `oha` (cargo install oha).
#
#   bench/run.sh [durée] [connexions]
#
# Le serveur et le générateur de charge sont épinglés sur des cœurs distincts
# (SERVER_CPUS / CLIENT_CPUS) pour ne pas se marcher dessus.
set -euo pipefail
cd "$(dirname "$0")/.."

DURATION=${1:-10s}
CONNECTIONS=${2:-128}
SERVER_CPUS=${SERVER_CPUS:-0,1}
CLIENT_CPUS=${CLIENT_CPUS:-2,3}
NCPU=$(awk -F, '{print NF}' <<<"$SERVER_CPUS")
RESULTS=$(mktemp -d)

command -v oha >/dev/null || { echo "oha introuvable : cargo install oha" >&2; exit 1; }
cargo build --release --example bench -q
(cd bench/axum && cargo build --release -q)
(cd bench/express && [ -d node_modules ] || npm install --silent --no-audit --no-fund)

SCENARIOS=(
  "texte|GET|/|"
  "json|GET|/json|"
  "params|GET|/users/42|"
  "post-json|POST|/echo|{\"name\":\"Ada\",\"langages\":[\"rust\",\"js\"],\"age\":36}"
)

wait_port() {
  for _ in $(seq 100); do
    curl -s -o /dev/null "http://127.0.0.1:$1/" && return 0
    sleep 0.1
  done
  echo "le serveur ne répond pas sur le port $1" >&2
  return 1
}

load() { # port méthode chemin corps durée
  local args=(-z "$5" -c "$CONNECTIONS" --no-tui --output-format json -m "$2")
  [ -n "$4" ] && args+=(-d "$4" -H "content-type: application/json")
  taskset -c "$CLIENT_CPUS" oha "${args[@]}" "http://127.0.0.1:$1$3"
}

run_server() { # nom port commande...
  local name=$1 port=$2
  shift 2
  taskset -c "$SERVER_CPUS" "$@" >/dev/null 2>&1 &
  local pid=$!
  wait_port "$port"
  load "$port" GET / "" 2s >/dev/null # échauffement (JIT de V8, caches…)
  for s in "${SCENARIOS[@]}"; do
    IFS='|' read -r label method path body <<<"$s"
    load "$port" "$method" "$path" "$body" "$DURATION" >"$RESULTS/$name.$label.json"
  done
  kill -TERM "$pid" 2>/dev/null || true
  pkill -P "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  sleep 0.5
}

echo "Serveur sur les CPU $SERVER_CPUS, oha sur $CLIENT_CPUS, $CONNECTIONS connexions, $DURATION par scénario"

run_server "Express (1 processus)" 3001 node bench/express/server.js
run_server "Express (cluster x$NCPU)" 3001 env WORKERS="$NCPU" node bench/express/server.js
run_server "axum 0.8" 3002 env WORKERS="$NCPU" PORT=3002 bench/axum/target/release/bench-axum
run_server "Vitesse" 3000 env WORKERS="$NCPU" PORT=3000 target/release/examples/bench
run_server "Vitesse (thread par cœur)" 3000 env VITESSE_MODE=tpc WORKERS="$NCPU" PORT=3000 target/release/examples/bench

python3 - "$RESULTS" <<'PY'
import json, os, sys
d = sys.argv[1]
servers, rows = [], {}
for f in sorted(os.listdir(d)):
    name, label, _ = f.rsplit(".", 2)
    s = json.load(open(os.path.join(d, f)))
    rps = s["summary"]["requestsPerSec"]
    p99 = s["latencyPercentiles"]["p99"] * 1000
    ok = s["statusCodeDistribution"]
    rows.setdefault(label, {})[name] = (rps, p99, ok)
    if name not in servers:
        servers.append(name)
order = ["texte", "json", "params", "post-json"]
rank = ["Express (1", "Express (cluster", "axum", "Vitesse", "Vitesse (thread"]
servers.sort(key=lambda n: max(i for i, p in enumerate(rank) if n.startswith(p)))
print()
print("| Scénario | " + " | ".join(servers) + " |")
print("|---|" + "---|" * len(servers))
for label in order:
    cells = []
    base = rows[label].get(servers[0], (1, 0, {}))[0]
    for n in servers:
        rps, p99, ok = rows[label][n]
        codes = ",".join(sorted(ok))
        warn = "" if codes == "200" else f" ⚠ {codes}"
        rps_txt = f"{rps:,.0f}".replace(",", "\u202f")
        cells.append(f"**{rps_txt}** req/s<br>p99 {p99:.2f} ms · ×{rps / base:.1f}{warn}")
    print(f"| {label} | " + " | ".join(cells) + " |")
PY
rm -rf "$RESULTS"
