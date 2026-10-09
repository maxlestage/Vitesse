// Serveur Express équivalent à `examples/bench.rs`.
// WORKERS=n lance n processus avec le module `cluster` (1 par défaut).
const cluster = require("node:cluster");
const express = require("express");

const workers = Number(process.env.WORKERS || 1);
const port = Number(process.env.PORT || 3001);

if (workers > 1 && cluster.isPrimary) {
  for (let i = 0; i < workers; i++) cluster.fork();
} else {
  const app = express();
  app.use(express.json());

  app.get("/", (req, res) => res.send("Hello, World!"));
  app.get("/json", (req, res) => res.json({ message: "Hello, World!" }));
  app.get("/users/:id", (req, res) => res.json({ id: req.params.id, name: "Ada" }));
  app.post("/echo", (req, res) => res.json(req.body));

  app.listen(port);
}
