import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export interface Parameter {
  name: string;
  value: any;
  section?: number;
  show?: boolean;
  type?: number;
  min?: number;
  max?: number;
  displayName?: string;
}

const parameters: Parameter[] = JSON.parse(
  fs.readFileSync(path.join(__dirname, "./data/parameters.json"), "utf8"),
);

export default parameters;
