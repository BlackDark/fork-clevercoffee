// Fails on user-facing English typed directly into Svelte templates. Text must come
// from Paraglide messages (`{m.key()}`); product names and symbols are allowed.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = new URL("../src", import.meta.url).pathname;
const ALLOWED = new Set([
  "CleverCoffee",
  "PID",
  "WiFi",
  "MQTT",
  "OTA",
  "BrewUI",
]);

function* svelteFiles(dir) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (name === "paraglide") continue;
    if (statSync(path).isDirectory()) yield* svelteFiles(path);
    else if (name.endsWith(".svelte")) yield path;
  }
}

// Blanks out comments, script and style blocks, keeping offsets for line numbers.
function blank(src) {
  return src.replace(
    /<!--[\s\S]*?-->|<(script|style)\b[\s\S]*?<\/\1>/g,
    (match) => match.replace(/[^\n]/g, " "),
  );
}

function lineOf(src, index) {
  return src.slice(0, index).split("\n").length;
}

// Strips `{...}` expressions, innermost first, so nested objects go too.
function stripExpressions(text) {
  let prev;
  do {
    prev = text;
    text = text.replace(/\{[^{}]*\}/g, " ");
  } while (text !== prev);
  return text;
}

function isEnglish(text) {
  const words = stripExpressions(text)
    .split(/[\s·—–-]+/)
    .filter((w) => /[A-Za-z]{2,}/.test(w) && !ALLOWED.has(w));
  return words.length > 0;
}

const problems = [];
for (const file of svelteFiles(ROOT)) {
  const src = blank(readFileSync(file, "utf8"));
  for (const match of src.matchAll(/(?<!=)>([^<>]*)</g)) {
    const text = match[1].trim();
    if (text && isEnglish(text)) {
      problems.push(
        `${relative(ROOT, file)}:${lineOf(src, match.index)}: text "${text.slice(0, 60)}"`,
      );
    }
  }
  for (const match of src.matchAll(
    /\b(aria-label|title|placeholder|alt)=(["'])([^"'{}]*)\2/g,
  )) {
    if (isEnglish(match[3])) {
      problems.push(
        `${relative(ROOT, file)}:${lineOf(src, match.index)}: ${match[1]} "${match[3].slice(0, 60)}"`,
      );
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
