
Prompt:

Given the following action plan (list of steps) for code optimization, refactoring, and improvement:

- Process the plan step by step.
- For each step, mark current active tasks as you work on them.
- After completing each task:
  - Summarize the changes as a changelog before proceeding.
  - Run the format script using  `~/.platformio/penv/bin/pio run --target format -e esp32_usb`.
  - Ensure the build by running  `~/.platformio/penv/bin/pio run -e esp32_usb`. A task is only considered successful if `build me` runs without errors.
- Do not mark a task as finished or start the next one unless both formatting was done and the build succeeded.
- After all steps are completed, and all conditions above are met, commit the code.
