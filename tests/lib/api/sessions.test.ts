import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { sessionsApi } from "@/lib/api/sessions";

describe("sessionsApi.previewSync", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("forces dryRun=true before invoking preview command", async () => {
    invokeMock.mockResolvedValueOnce({
      totalScanned: 1,
      imported: 1,
      skipped: 0,
      conflicts: 0,
      failed: 0,
      warnings: [],
    });

    await sessionsApi.previewSync({
      targetProviderId: "claude",
      sourceProviderIds: ["codex"],
      dryRun: false,
    });

    expect(invokeMock).toHaveBeenCalledWith("preview_session_sync", {
      request: {
        targetProviderId: "claude",
        sourceProviderIds: ["codex"],
        dryRun: true,
      },
    });
  });

  it("invokes sync capability endpoint", async () => {
    invokeMock.mockResolvedValueOnce({
      executionSupported: true,
      reason: "ok",
      supportedTargetProviders: ["codex", "claude", "gemini"],
    });

    await sessionsApi.getSyncCapabilities("codex");

    expect(invokeMock).toHaveBeenCalledWith("get_session_sync_capabilities", {
      targetProviderId: "codex",
    });
  });
});
