const fs = require("fs");
const path = require("path");

// Function to convert dot notation key to camelCase variable name
function keyToCamelCase(key) {
  return key
    .split(".")
    .map((part, index) => {
      // Convert underscores to camelCase within each part
      const camelCasePart = part.replace(/_([a-z])/g, (match, letter) =>
        letter.toUpperCase()
      );
      // Capitalize first letter of each part except the first one
      return index === 0
        ? camelCasePart
        : camelCasePart.charAt(0).toUpperCase() + camelCasePart.slice(1);
    })
    .join("");
}

// Function to process the file
function processConfigFile(filePath) {
  try {
    // Read the file
    let content = fs.readFileSync(filePath, "utf8");

    // Regular expression to match parameter definitions
    // Matches: ParamDef<type> variableName{"key.with.dots"
    // or: EnumParamDef<type> variableName{"key.with.dots"
    const paramDefRegex =
      /((?:Param|Enum)Def<[^>]+>)\s+(\w+)\s*\{\s*"([^"]+)"/g;

    let match;
    const replacements = [];
    const allVariables = [];

    // Find all matches and prepare replacements
    while ((match = paramDefRegex.exec(content)) !== null) {
      const fullMatch = match[0];
      const defType = match[1]; // ParamDef<bool>, EnumParamDef<Hardware::SwitchType>, etc.
      const currentVarName = match[2]; // current variable name
      const key = match[3]; // the key like "hardware.leds.status.enabled"

      // Generate new variable name from key
      const newVarName = keyToCamelCase(key);

      // Add to all variables list (use new name if different, otherwise current)
      allVariables.push(
        currentVarName !== newVarName ? newVarName : currentVarName
      );

      // Only add to replacements if the names are different
      if (currentVarName !== newVarName) {
        const newMatch = fullMatch.replace(currentVarName, newVarName);
        replacements.push({
          original: fullMatch,
          replacement: newMatch,
          oldName: currentVarName,
          newName: newVarName,
          key: key,
        });
      }
    }

    // Apply replacements
    let modifiedContent = content;
    for (const replacement of replacements) {
      modifiedContent = modifiedContent.replace(
        replacement.original,
        replacement.replacement
      );
      console.log(
        `Renamed: ${replacement.oldName} -> ${replacement.newName} (key: ${replacement.key})`
      );
    }

    // Write the modified content back to file
    fs.writeFileSync(filePath, modifiedContent, "utf8");

    console.log(`\nProcessed ${replacements.length} parameter definitions.`);
    console.log(`File updated: ${filePath}`);
    console.log(`\nAll variable names:\n`);

    // Print all variables to console, one per line
    allVariables.forEach((varName) => {
      console.log(varName);
    });
  } catch (error) {
    console.error("Error processing file:", error.message);
  }
}

// Main execution
function main() {
  const args = process.argv.slice(2);

  if (args.length === 0) {
    console.log("Usage: node rename-params.js <path-to-config-file>");
    console.log("Example: node rename-params.js Config.h");
    process.exit(1);
  }

  const filePath = args[0];

  if (!fs.existsSync(filePath)) {
    console.error(`File not found: ${filePath}`);
    process.exit(1);
  }

  console.log(`Processing file: ${filePath}`);
  processConfigFile(filePath);
}

// Run the script
main();
