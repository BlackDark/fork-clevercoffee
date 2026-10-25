import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import ParameterInput from "./ParameterInput.svelte";

const parameter = {
  name: "brew.setpoint",
  type: 2 as const,
  value: 93,
  defaultValue: 93,
  min: 0,
  max: 150,
};
afterEach(cleanup);
describe("typed parameter controls", () => {
  it("emits a number rather than a string", async () => {
    const onchange = vi.fn();
    const { getByRole } = render(ParameterInput, { parameter, onchange });
    await fireEvent.change(getByRole("spinbutton"), {
      target: { value: "94.5" },
    });
    expect(onchange).toHaveBeenCalledWith(94.5);
  });
  it("emits boolean values from the native switch", async () => {
    const onchange = vi.fn();
    const { getByRole } = render(ParameterInput, {
      parameter: { ...parameter, type: 6, value: false, min: 0, max: 1 },
      onchange,
    });
    await fireEvent.click(getByRole("switch"));
    expect(onchange).toHaveBeenCalledWith(true);
  });
  it("keeps dependent controls disabled", () => {
    const { getByRole } = render(ParameterInput, {
      parameter,
      disabled: true,
      onchange: vi.fn(),
    });
    expect(getByRole("spinbutton")).toBeDisabled();
  });
});

it("does not put an empty numeric value into the draft", async () => {
  const onchange = vi.fn();
  const { getByRole } = render(ParameterInput, { parameter, onchange });
  await fireEvent.change(getByRole("spinbutton"), { target: { value: "" } });
  expect(onchange).not.toHaveBeenCalled();
});

it("uses the visible label as the accessible name", () => {
  const { getByRole } = render(ParameterInput, {
    parameter,
    label: "Brew temperature",
    onchange: vi.fn(),
  });
  expect(getByRole("spinbutton", { name: "Brew temperature" })).toBeVisible();
});
