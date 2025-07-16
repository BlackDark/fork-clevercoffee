import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Load parameters from example_parameters.json
const parameters = JSON.parse(
  fs.readFileSync(
    path.join(__dirname, "../ui/data/example_parameters.json"),
    "utf8"
  )
);

export default parameters;
