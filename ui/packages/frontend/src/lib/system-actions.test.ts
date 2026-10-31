import { afterEach, describe, expect, it, vi } from "vitest";
import { postDeviceAction, uploadConfig } from "./system-actions";

const reply = (body: string, status = 200) =>
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue(new Response(body, { status })),
  );
const json = (text: string, name = "config.json") =>
  new File([text], name, { type: "application/json" });

afterEach(() => vi.unstubAllGlobals());
describe("device actions", () => {
  it("accepts a reply without a JSON body", async () => {
    reply("OK");
    await expect(postDeviceAction("restart")).resolves.toContain("accepted");
  });
  it("reports a refusal and HTTP errors", async () => {
    reply('{"success":false}');
    await expect(postDeviceAction("restart")).rejects.toThrow("refused");
    reply("busy", 503);
    await expect(postDeviceAction("restart")).rejects.toThrow("HTTP 503: busy");
  });
});
describe("configuration upload", () => {
  it("rejects files before sending anything", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    await expect(uploadConfig(null)).rejects.toThrow("nonempty JSON");
    await expect(uploadConfig(json("{}", "config.txt"))).rejects.toThrow(
      "nonempty JSON",
    );
    await expect(uploadConfig(json("[]"))).rejects.toThrow("JSON object");
    expect(fetchMock).not.toHaveBeenCalled();
  });
  it("returns the device message, or a default", async () => {
    reply('{"success":true,"message":"Restored"}');
    await expect(uploadConfig(json("{}"))).resolves.toBe("Restored");
    reply('{"success":true}');
    await expect(uploadConfig(json("{}"))).resolves.toContain("restored");
  });
  it("explains invalid replies", async () => {
    reply("not json");
    await expect(uploadConfig(json("{}"))).rejects.toThrow(
      "Invalid configuration upload response (HTTP 200)",
    );
    reply('{"success":false,"message":"Bad key"}');
    await expect(uploadConfig(json("{}"))).rejects.toThrow("Bad key");
  });
});
