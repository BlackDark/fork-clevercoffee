<GENERAL>
I want you to analysis the complete code base (include files and src files) and check every file for things like optimization, structual correctness, best practices, usage of modern features, legacy code which should be removed, testability and most important unnecessary complex logic. Removal of files is desired to make the structures and flow easier to understand.

Take your time and thorougly analysis the code. Make yourself also aware of CLAUDE.md which describes the basics a bit.

Create a detailed plan of what could be improved, what is bad and what you would recommend. After that we can discuss it and if we agree you can start creating a detailed implemenation plan.

Ask me questions if needed
</GENERAL>

<RULES>

- never create and implement backward compatiblity code. Remove it
- never maintain legacy layers. Remove it
- after a completed task make sure the build runs
- do not write unnecessary documentation. Only what is necessary
- try not to create to many new files. We want to reduce complexity

</RULES>

<TASK>

the brew does not continue:

[00:00:26] [INFO] [HotWaterHandler] Hot water momentary switch pressed
[00:00:28] [INFO] Disabling pump
[00:00:28] [INFO] [HotWaterHandler] Hot water momentary switch released
[00:00:30] [INFO] Resetting standby timer to 35 minutes
[00:00:35] [INFO] [BrewHandler] Brew momentary switch pressed
[00:00:35] [INFO] State transition: 20 -> 31 (Brew start requested)
[00:00:35] [INFO] State transition: 20 (PID Normal) -> 31 (Brew Preinfusion) [State transition]
[00:00:35] [INFO] State transition: 20 -> 31 (State transition)
[00:00:35] [INFO] Exiting state 20 (PID Normal)
[00:00:35] [INFO] Entering state 31 (Brew Preinfusion)
[00:00:35] [INFO] Brew preinfusion started
[00:00:35] [WARNING] State transition detected: 20 -> 31 (PID Normal -> Brew Preinfusion)
[00:00:39] [INFO] [BrewHandler] Brew momentary switch released

no further logs

</TASK>


