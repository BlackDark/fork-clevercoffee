# Error Handling Policy

## Principles

1. **Use exceptions for exceptional conditions**
   - Initialization failures
   - Hardware failures that prevent operation
   - Critical system errors

2. **Use Expected<T, Error> for recoverable errors**
   - Sensor read failures (with cached values)
   - Network operation failures
   - Validation failures

3. **Use bool return for trivial operations**
   - Set operations where success/failure is enough info
   - Non-critical updates

4. **Never ignore errors**
   - All error paths must be handled
   - Log errors appropriately
