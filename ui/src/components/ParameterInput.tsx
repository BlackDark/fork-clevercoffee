import React from "react";
import {
  Parameter,
  isBoolean,
  getInputType,
  getNumberStep,
  validateParameterValue,
} from "../lib/parameter-utils";

interface ParameterInputProps {
  parameter: Parameter;
  value: any;
  onChange: (value: any) => void;
  onHelpClick?: (parameterName: string) => void;
  disabled?: boolean;
}

export function ParameterInput({
  parameter,
  value,
  onChange,
  onHelpClick,
  disabled = false,
}: ParameterInputProps) {
  const isValid = validateParameterValue(parameter, value);
  const inputType = getInputType(parameter);

  const handleChange = (
    event: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>
  ) => {
    let newValue: any = event.target.value;

    // Convert to appropriate type
    switch (parameter.type) {
      case 0: // integer
      case 1: // uint8
        newValue = parseInt(newValue, 10);
        break;
      case 2: // double
      case 3: // float
        newValue = parseFloat(newValue);
        break;
      case 5: // enum
        newValue = parseInt(newValue, 10);
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
        {parameter.displayName}
        {parameter.hasHelpText && onHelpClick && (
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
            value={value}
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
            value={value}
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
            value={value}
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
