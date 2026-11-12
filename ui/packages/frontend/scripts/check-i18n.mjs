// Fails on user-facing English typed directly into Svelte templates. Text must come
// from Paraglide messages (`{m.key()}`); product names and symbols are allowed.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { parse } from "svelte/compiler";

const ROOT = fileURLToPath(new URL("../src", import.meta.url));
const ALLOWED = new Set([
  "CleverCoffee",
  "PID",
  "WiFi",
  "MQTT",
  "OTA",
  "BrewUI",
]);
const TEXT_ATTRIBUTES = new Set(["aria-label", "title", "placeholder", "alt"]);

function* svelteFiles(dir) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (name === "paraglide") continue;
    if (statSync(path).isDirectory()) yield* svelteFiles(path);
    else if (name.endsWith(".svelte")) yield path;
  }
}

// Every node in the template tree. Attribute values are yielded but not entered:
// they are checked by name in `attributeProblems`.
function* walk(node) {
  if (Array.isArray(node)) {
    for (const child of node) yield* walk(child);
    return;
  }
  if (!node || typeof node !== "object") return;
  yield node;
  if (node.type === "Attribute") return;
  for (const value of Object.values(node)) {
    if (value && typeof value === "object") yield* walk(value);
  }
}

function isEnglish(text) {
  return text
    .split(/[\s·—–-]+/)
    .some((w) => /[A-Za-z]{2,}/.test(w) && !ALLOWED.has(w));
}

// String literals inside a JS expression, e.g. `{c ? "Open" : m.x()}`.
function* stringLiterals(node) {
  for (const n of walk(node)) {
    if (n.type === "Literal" && typeof n.value === "string") yield n.value;
  }
}

function attributeProblems(attribute) {
  if (!TEXT_ATTRIBUTES.has(attribute.name) || attribute.value === true)
    return [];
  const parts = Array.isArray(attribute.value)
    ? attribute.value
    : [attribute.value];
  const found = [];
  for (const part of parts) {
    if (part.type === "Text" && isEnglish(part.data)) found.push(part.data);
    if (part.type === "ExpressionTag") {
      // Lowercase single words are comparison keys (`action === "upload"`), not text.
      for (const literal of stringLiterals(part.expression)) {
        if (/^[A-Z]|\s/.test(literal) && isEnglish(literal))
          found.push(literal);
      }
    }
  }
  return found.map((text) => `${attribute.name} "${text.trim().slice(0, 60)}"`);
}

const problems = [];
for (const file of svelteFiles(ROOT)) {
  const src = readFileSync(file, "utf8");
  const where = (node) =>
    `${relative(ROOT, file)}:${src.slice(0, node.start).split("\n").length}`;
  for (const node of walk(parse(src, { modern: true }).fragment)) {
    if (node.type === "Text" && isEnglish(node.data)) {
      problems.push(`${where(node)}: text "${node.data.trim().slice(0, 60)}"`);
    } else if (node.type === "Attribute") {
      for (const message of attributeProblems(node)) {
        problems.push(`${where(node)}: ${message}`);
      }
    }
  }
}

if (problems.length) {
  console.error(problems.join("\n"));
  console.error(
    `\n${problems.length} hard-coded string(s): use Paraglide messages (src/paraglide).`,
  );
  process.exit(1);
}
console.log("i18n: no hard-coded UI text");
