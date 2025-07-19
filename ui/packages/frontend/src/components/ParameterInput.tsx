import React from "react";
import type { Parameter } from "../lib/parameter-types";
import {
  isBoolean,
  getInputType,
  getNumberStep,
  validateParameterValue,
} from "../lib/parameter-utils";
import { getParameterLabel } from "../lib/parameter-labels";
import { parameterHelpTexts } from "../lib/parameter-help-texts";

interface ParameterInputProps {
  parameter: Parameter;
  value: string | number | boolean;
  onChange: (value: string | number | boolean) => void;
  onHelpClick?: (parameterName: string) => void;
  disabled?: boolean;
  disabledHint?: string;
}

export function ParameterInput({
  parameter,
  value,
  onChange,
  onHelpClick,
  disabled = false,
  disabledHint,
}: ParameterInputProps) {
  const isValid = validateParameterValue(parameter, value);
  const inputType = getInputType(parameter);

  const handleChange = (
    event: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>
  ) => {
    let newValue: string | number | boolean = event.target.value;

    // Convert to appropriate type
    switch (parameter.type) {
      case 0: // integer
      case 1: // uint8
        newValue = parseInt(newValue as string, 10);
        break;
      case 2: // double
      case 3: // float
        newValue = parseFloat(newValue as string);
        break;
      case 5: // enum
        newValue = parseInt(newValue as string, 10);
        break;
      // string (type 4) stays as string
    }

    onChange(newValue);
  };

  const handleCheckboxChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    onChange(event.target.checked ? 1 : 0);
  };

  return (
    <div className="mb-3">
      <label
        className="form-label d-flex align-items-center gap-2"
        htmlFor={`param-${parameter.name}`}
      >
        {getParameterLabel(parameter.name)}
        {parameterHelpTexts[parameter.name] && onHelpClick && (
          <button
            type="button"
            className="btn btn-link p-0 text-info"
            onClick={() => onHelpClick(parameter.name)}
            title="Show help"
          >
            <i className="fas fa-question-circle"></i>
          </button>
        )}
      </label>

      {/* Show disabled hint if parameter is disabled */}
      {disabled && disabledHint && (
        <div className="alert alert-warning alert-sm mb-2">
          <i className="fas fa-exclamation-triangle me-2"></i>
          {disabledHint}
        </div>
      )}

      {isBoolean(parameter) ? (
        // Boolean parameter (checkbox)
        <div className="form-check">
          <input
            type="checkbox"
            className="form-check-input"
            id={`param-${parameter.name}`}
            checked={value === 1}
            onChange={handleCheckboxChange}
            disabled={disabled}
          />
        </div>
      ) : parameter.type === 5 && parameter.options ? (
        // Enum parameter (select)
        <div>
          <select
            className={`form-select ${!isValid ? "is-invalid" : ""}`}
            id={`param-${parameter.name}`}
            value={String(value)}
            onChange={handleChange}
            disabled={disabled}
          >
            {parameter.options.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
          {!isValid && (
            <div className="invalid-feedback">
              Value must be between {parameter.min} and {parameter.max}
            </div>
          )}
        </div>
      ) : parameter.type === 4 ? (
        // String parameter
        <div>
          <input
            type={inputType}
            className={`form-control ${!isValid ? "is-invalid" : ""}`}
            id={`param-${parameter.name}`}
            value={String(value)}
            onChange={handleChange}
            maxLength={parameter.max > 0 ? parameter.max : undefined}
            disabled={disabled}
          />
          {!isValid && (
            <div className="invalid-feedback">
              {parameter.max > 0 && String(value).length > parameter.max
                ? `Maximum length is ${parameter.max} characters`
                : "Invalid value"}
            </div>
          )}
        </div>
      ) : (
        // Numeric parameter
        <div>
          <input
            type={inputType}
            className={`form-control ${!isValid ? "is-invalid" : ""}`}
            id={`param-${parameter.name}`}
            value={String(value)}
            onChange={handleChange}
            min={parameter.min}
            max={parameter.max}
            step={getNumberStep(parameter)}
            disabled={disabled}
          />
          {!isValid && (
            <div className="invalid-feedback">
              Value must be between {parameter.min} and {parameter.max}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
