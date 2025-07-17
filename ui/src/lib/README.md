# Parameter Library Refactoring

## Overview
This refactoring consolidates the duplicated parameter management code in the `ui/src/lib` directory into a more maintainable and organized structure.

## Changes Made

### Files Removed (Duplicates)
- `all-parameters.ts` - Merged into `parameter-definitions.ts`
- `complete-parameters.ts` - Merged into `parameter-definitions.ts`

### New Unified Structure

#### Core Files
1. **`parameter-types.ts`** - Unified type definitions
   - `ParameterTypes` constants
   - `Parameter` interface
   - `ParameterCondition` interface
   - Type helper functions

2. **`parameter-definitions.ts`** - Consolidated parameter definitions
   - All parameter definitions with default values
   - Condition logic
   - Utility functions for merging with server data

3. **`parameter-utils.ts`** - Unified utility functions
   - Parameter visibility evaluation
   - Validation functions
   - Formatting and conversion utilities
   - Search and sorting functions

#### Supporting Files
4. **`parameter-conditions.ts`** - Backward compatibility exports
5. **`parameter-help-texts.ts`** - Help text definitions (unchanged)
6. **`parameter-groups.ts`** - Parameter grouping logic (unchanged)
7. **`parameter-labels.ts`** - Localization labels (cleaned up duplicates)
8. **`index.ts`** - Single entry point for all exports

#### Utility Files (Unchanged)
- `api-config.ts` - API configuration
- `button-variants.ts` - Button styling variants
- `utils.ts` - General utilities

## Benefits

### 1. Eliminated Duplications
- Removed ~2800 lines of duplicated parameter definitions
- Consolidated multiple `Parameter` interface definitions
- Unified condition evaluation logic

### 2. Improved Type Safety
- Strict typing with no `any` types
- Consistent interfaces across all files
- Better TypeScript support

### 3. Better Organization
- Single source of truth for parameter definitions
- Clear separation of concerns
- Logical file structure

### 4. Easier Maintenance
- Changes only need to be made in one place
- Consistent parameter handling
- Better code reusability

## Migration Guide

### For Existing Code

#### Old imports:
```typescript
import { allParameters } from "./lib/all-parameters";
import { completeParameterDefinitions } from "./lib/complete-parameters";
import { evaluateParameterConditions } from "./lib/parameter-conditions";
```

#### New imports:
```typescript
import { parameterDefinitions, mergeParametersWithDefaults } from "./lib/parameter-definitions";
import { shouldShowParameter } from "./lib/parameter-utils";
// Or use the consolidated import:
import { parameterDefinitions, shouldShowParameter, mergeParametersWithDefaults } from "./lib";
```

#### Function name changes:
- `evaluateParameterConditions()` → `shouldShowParameter()`
- `allParameters` → `parameterDefinitions`
- `completeParameterDefinitions` → `parameterDefinitions`

### Type Updates
All parameter-related types are now imported from `parameter-types.ts`:
```typescript
import type { Parameter, ParameterCondition, ParameterType } from "./lib/parameter-types";
```

## File Structure
```
ui/src/lib/
├── index.ts                    # Main exports
├── parameter-types.ts          # Core type definitions
├── parameter-definitions.ts    # Parameter data
├── parameter-utils.ts          # Utility functions
├── parameter-conditions.ts     # Backward compatibility
├── parameter-help-texts.ts     # Help text definitions
├── parameter-groups.ts         # Parameter grouping
├── parameter-labels.ts         # Localization labels
├── api-config.ts              # API configuration
├── button-variants.ts          # UI components
└── utils.ts                   # General utilities
```

## Next Steps
1. Update component imports to use the new structure
2. Test all parameter-related functionality
3. Remove any remaining references to deleted files
4. Consider adding unit tests for the new utility functions
