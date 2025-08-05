const fs = require("fs");
const path = require("path");

function convertConfigKey(key) {
  // Convert dot notation and underscores to camelCase
  return key.replace(/[._]([a-z])/g, (_, letter) => letter.toUpperCase());
}

function processFile(filePath) {
  try {
    // Read file content
    const content = fs.readFileSync(filePath, "utf8");

    // Regex to match Config::getInstance().get<TYPE>("key") pattern
    const regex = /Config::getInstance\(\)\.get<[^>]+>\("([^"]+)"\)/g;

    // Replace with the new format
    const newContent = content.replace(regex, (match, key) => {
      const camelCaseKey = convertConfigKey(key);
      return `Config::getInstance().${camelCaseKey}.get()`;
    });

    // Only write back if content changed
    if (content !== newContent) {
      fs.writeFileSync(filePath, newContent, "utf8");
      console.log(`✓ Updated: ${filePath}`);

      // Log the replacements made
      const matches = content.match(regex);
      if (matches) {
        matches.forEach((match) => {
          const key = match.match(/"([^"]+)"/)[1];
          const camelCaseKey = convertConfigKey(key);
          console.log(
            `  ${match} → Config::getInstance().${camelCaseKey}.get()`
          );
        });
      }
    }
  } catch (error) {
    console.error(`Error processing file ${filePath}:`, error.message);
  }
}

function traverseDirectory(dir) {
  try {
    const items = fs.readdirSync(dir, { withFileTypes: true });

    for (const item of items) {
      const fullPath = path.join(dir, item.name);

      if (item.isDirectory()) {
        // Recursively traverse subdirectories
        traverseDirectory(fullPath);
      } else if (item.isFile()) {
        // Process files (you can add file extension filters here)
        const ext = path.extname(item.name).toLowerCase();

        // Only process certain file types (adjust as needed)
        if ([".cpp", ".h", ".hpp", ".c", ".cc", ".cxx"].includes(ext)) {
          processFile(fullPath);
        }
      }
    }
  } catch (error) {
    console.error(`Error reading directory ${dir}:`, error.message);
  }
}

// Main execution
function main() {
  const srcDir = "./src";

  // Check if src directory exists
  if (!fs.existsSync(srcDir)) {
    console.error(`Directory "${srcDir}" does not exist!`);
    process.exit(1);
  }

  console.log(`Starting regex replacement in directory: ${srcDir}`);
  console.log("=".repeat(50));

  traverseDirectory(srcDir);

  console.log("=".repeat(50));
  console.log("Replacement complete!");
}

// Run the script
main();
