#!/usr/bin/env bash
# Benchmark Vitesse vs actix-web, axum, Drogon et Express avec `wrk`.
#
#   bench/run.sh [durée] [connexions]
#
# Prérequis : wrk, Node.js, et Drogon installé pour bench/drogon (sinon il
# est ignoré ; DROGON_PREFIX=/chemin/install si besoin).
#
# Le serveur et wrk sont épinglés sur des cœurs distincts (SERVER_CPUS /
# CLIENT_CPUS). En plus du débit, on mesure le temps CPU consommé par le
# serveur pour chaque requête : une mesure qui ne dépend pas de wrk.
set -euo pipefail
cd "$(dirname "$0")/.."

DURATION=${1:-10s}
CONNECTIONS=${2:-128}
SERVER_CPUS=${SERVER_CPUS:-0,1}
CLIENT_CPUS=${CLIENT_CPUS:-2,3}
NCPU=$(awk -F, '{print NF}' <<<"$SERVER_CPUS")
WRK=${WRK:-wrk}
RESULTS=$(mktemp -d)
trap 'rm -rf "$RESULTS"' EXIT

command -v "$WRK" >/dev/null || { echo "wrk introuvable (apt install wrk / brew install wrk)" >&2; exit 1; }
cargo build --release --example bench -q
(cd bench/axum && cargo build --release -q)
(cd bench/actix && cargo build --release -q)
(cd bench/express && [ -d node_modules ] || npm install --silent --no-audit --no-fund)
if [ ! -x bench/drogon/build/bench-drogon ]; then
  cmake -S bench/drogon -B bench/drogon/build -DCMAKE_BUILD_TYPE=Release \
    ${DROGON_PREFIX:+-DCMAKE_PREFIX_PATH=$DROGON_PREFIX} >/dev/null 2>&1 &&
    cmake --build bench/drogon/build -j >/dev/null 2>&1 ||
    echo "Drogon introuvable : bench/drogon ignoré" >&2
fi

# nom|chemin|script|argument
SCENARIOS=(
  "texte|/||"
  "json|/json||"
  "navigateur|/json|bench/lua/navigateur.lua|"
  "params|/users/42||"
  "post-json|/echo|bench/lua/post.lua|"
  "pipeline×16|/|bench/lua/pipeline.lua|16"
)

wait_port() {
  for _ in $(seq 100); do
    curl -s -o /dev/null "http://127.0.0.1:$1/" && return 0
    sleep 0.1
  done
  echo "le serveur ne répond pas sur le port $1" >&2
  return 1
}

# Temps CPU (en centièmes de seconde) d'un processus et de ses enfants.
cpu_ticks() {
  local total=0 pid
  for pid in $1 $(pgrep -P "$1" || true); do
    total=$((total + $(awk '{print $14 + $15}' "/proc/$pid/stat" 2>/dev/null || echo 0)))
  done
  echo "$total"
}

load() { # port chemin script argument durée
  local args=(-t"$NCPU" -c"$CONNECTIONS" -d"$5" --latency)
  [ -n "$3" ] && args+=(-s "$3")
  taskset -c "$CLIENT_CPUS" "$WRK" "${args[@]}" "http://127.0.0.1:$1$2" -- $4
}

run_server() { # nom port commande...
  local name=$1 port=$2
  shift 2
  taskset -c "$SERVER_CPUS" "$@" >/dev/null 2>&1 &
  local pid=$!
  wait_port "$port"
  load "$port" / "" "" 2s >/dev/null # échauffement (JIT de V8, caches…)
  for s in "${SCENARIOS[@]}"; do
    IFS='|' read -r label path script arg <<<"$s"
    local before after
    before=$(cpu_ticks "$pid")
    load "$port" "$path" "$script" "$arg" "$DURATION" >"$RESULTS/out"
    after=$(cpu_ticks "$pid")
    {
      echo "$name|$label|$((after - before))"
      cat "$RESULTS/out"
    } >"$RESULTS/$name.$label"
  done
  kill -TERM "$pid" 2>/dev/null || true
  pkill -TERM -P "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  sleep 0.5
}

echo "Serveur sur les CPU $SERVER_CPUS, wrk sur $CLIENT_CPUS, $CONNECTIONS connexions, $DURATION par scénario"

run_server "Express" 3001 node bench/express/server.js
run_server "Express cluster" 3001 env WORKERS="$NCPU" node bench/express/server.js
[ -x bench/drogon/build/bench-drogon ] &&
  run_server "Drogon" 3003 env WORKERS="$NCPU" PORT=3003 bench/drogon/build/bench-drogon
run_server "axum" 3002 env WORKERS="$NCPU" PORT=3002 bench/axum/target/release/bench-axum
run_server "actix-web" 3004 env WORKERS="$NCPU" PORT=3004 bench/actix/target/release/bench-actix
run_server "Vitesse" 3000 env WORKERS="$NCPU" PORT=3000 target/release/examples/bench

python3 - "$RESULTS" <<'PY'
import os, re, sys

d = sys.argv[1]
rows, servers = {}, []
for f in sorted(os.listdir(d)):
    if f == "out":
        continue
    text = open(os.path.join(d, f)).read()
    name, label, ticks = text.splitlines()[0].split("|")
    reqs = int(re.search(r"(\d+) requests in", text).group(1))
    rps = float(re.search(r"Requests/sec:\s+([\d.]+)", text).group(1))
    errors = re.search(r"Non-2xx or 3xx responses: (\d+)", text)
    rows.setdefault(label, {})[name] = (rps, int(ticks) * 1e4 / reqs, errors)
    if name not in servers:
        servers.append(name)

order = ["Express", "Express cluster", "Drogon", "axum", "actix-web", "Vitesse"]
servers.sort(key=order.index)
fmt = lambda n: f"{n:,.0f}".replace(",", " ")
print()
print("Requêtes par seconde (temps CPU serveur par requête) :")
print()
print("| Scénario | " + " | ".join(servers) + " |")
print("|---|" + "---:|" * len(servers))
for label in ["texte", "json", "navigateur", "params", "post-json", "pipeline×16"]:
    cells = []
    for name in servers:
        rps, us, errors = rows[label][name]
        cell = f"{fmt(rps)} ({us:.2f} µs)"
        if name == "Vitesse":
            cell = f"**{cell}**"
        if errors:
            cell += f" ⚠ {errors.group(1)} erreurs"
        cells.append(cell)
    print(f"| {label} | " + " | ".join(cells) + " |")
PY
