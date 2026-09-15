#!/usr/bin/env node
// Drives the wasm engine the way a browser page does: load a compiled module, present each
// command, answer it. Prints what it presented, so a caller can diff it against the native run.
//
// Usage: wasm-smoke.cjs <glue-dir> <module.velac>
//
// The point of the diff is that it is the *same engine* on both sides. If the browser build
// played a different story from the native one, every claim about determinism would stop at the
// edge of the platform — which is exactly what `RUNTIME.md §4.1` says cannot happen.

const fs = require("node:fs");
const path = require("node:path");

const [glue, modulePath] = process.argv.slice(2);
if (!glue || !modulePath) {
  process.stderr.write("usage: wasm-smoke.cjs <glue-dir> <module.velac>\n");
  process.exit(2);
}

const { Player } = require(path.resolve(glue, "vela_web.js"));
const player = new Player(fs.readFileSync(modulePath), "start");

// `step` presents the first command; every later one comes back from the answer, which is why
// the loop answers before asking again rather than calling `step` twice.
let command = player.step();
let presented = 0;

while (command !== "") {
  if (command.startsWith("fault: ")) {
    process.stderr.write(`${command}\n`);
    process.exit(1);
  }
  process.stdout.write(`${command}\n`);
  presented += 1;

  // A menu is answered with a choice and dialogue with an acknowledgement. Reading the command
  // to decide is what a page does too, until the focus stack is wired up.
  command = command.startsWith("menu") ? player.choose(0) : player.line();
}

if (presented === 0) {
  process.stderr.write("the story presented nothing\n");
  process.exit(1);
}
