# Project: UI Migration

- you can run "pnpm lint:fix" after you changed some things to lint the code
- after you finished a bigger change "pnpm lint" and "pnpm check" should work without fail
- do not modify the src/lib/components/ui folder. Those are auto generated things by shadcn. If modififcations would be need write them down in a file. If components do not exists exactly the same check the shadcn docs for how to use the components
- adjust the .ts files only when necessary they are mostly correct and working and the errors mostly are in the components and they need adjustments. If you really think something is wrong the in state files or .ts files, ask me
- Feel free to validate with "pnpm build" and "pnpm check" if the svelte project works.

## Useful Command-Line Tools

### GitHub
- Use the `gh` command-line to interact with GitHub.

### JSON
- Use the `jq` command to read and extract information from JSON files.

### RipGrep
- The `rg` (ripgrep) command is available for fast searches in text files.

### Clipboard
- Pipe content into `pbcopy` to copy it into the clipboard. Example: `echo "hello" | pbcopy`.
- Pipe from `pbpaste` to get the contents of the clipboard. Example: `pbpaste > fromclipboard.txt`.

### Python
- Unless instructed otherwise, always use the `uv` Python environment and package manager for Python.
  - `uv run ...` for running a python script.
  - `uvx ...` for running program directly from a PyPI package.
  - `uv ... ...` for managing environments, installing packages, etc...

### JavaScript / TypeScript
- Use `pnpx` for running commands directly from npm packages.
- add types where needed and do not use any

### Svelte

- use always Svelte 5 Syntax
- use runes and avoid store. If you see store rewrite it to use runes. Only use stores if absolutely necessary and add a comment why

## Documentation Sources
- If working with a new library or tool, consider looking for its documentation from its website, GitHub project, or the relevant llms.txt.
  - It is always better to have accurate, up-to-date documentation at your disposal, rather than relying on your pre-trained knowledge.
- You can search the following directories for llms.txt collections for many projects:
  - https://llmstxt.site/
  - https://directory.llmstxt.cloud/
- If you find a relevant llms.txt file, follow the links until you have access to the complete documentation.

## General Instructions:

- When generating new TypeScript code, please follow the existing coding style.
- Add documents only where necessary
- Prefer functional programming paradigms where appropriate.
- All code should be compatible with TypeScript 5.0 and Node.js 22+.
- Build and test the code

## Coding Style:

- Use 2 spaces for indentation.
- Always use strict equality (`===` and `!==`).

## Regarding Dependencies:

- Avoid introducing new external dependencies unless absolutely necessary.
- If a new dependency is required, please state the reason.
