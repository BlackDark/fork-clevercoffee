import { describe, expect, it } from "vitest";
import { parameterGroups } from "./parameter-groups";
import { defaultParametersList } from "./parameter-metadata";

describe("parameter metadata", () => {
  it("uses canonical brew parameter keys", () => {
    const names = defaultParametersList.map((param) => param.name);

    expect(names).not.toContain("brew.target_time");
    expect(names).not.toContain("brew.target_weight");
    expect(names).toContain("brew.by_time.target_time");
    expect(names).toContain("brew.by_weight.target_weight");
    expect(names).toContain("brew.by_weight.auto_tare");
  });

  it("defines metadata for every parameter shown in config groups", () => {
    const metadataNames = new Set(
      defaultParametersList.map((param) => param.name),
    );
    const groupNames = parameterGroups.flatMap((group) => group.parameters);
    const missing = groupNames.filter((name) => !metadataNames.has(name));

    expect(missing, `Missing metadata for: ${missing.join(", ")}`).toEqual([]);
  });
});
