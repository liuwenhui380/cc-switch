import { renderHook, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { useSessionSync } from "@/hooks/useSessionSync";

const toastSuccessMock = vi.fn();
const toastErrorMock = vi.fn();
const toastWarningMock = vi.fn();

vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: (...args: unknown[]) => toastErrorMock(...args),
    warning: (...args: unknown[]) => toastWarningMock(...args),
  },
}));

const previewMutateAsyncMock = vi.fn();
const syncMutateAsyncMock = vi.fn();

vi.mock("@/lib/query", () => ({
  usePreviewSessionSyncMutation: () => ({
    mutateAsync: previewMutateAsyncMock,
    isPending: false,
  }),
  useSyncSessionsToProviderMutation: () => ({
    mutateAsync: syncMutateAsyncMock,
    isPending: false,
  }),
}));

describe("useSessionSync", () => {
  beforeEach(() => {
    previewMutateAsyncMock.mockReset();
    syncMutateAsyncMock.mockReset();
    toastSuccessMock.mockReset();
    toastErrorMock.mockReset();
    toastWarningMock.mockReset();
  });

  it("enforces dryRun=true when previewing", async () => {
    previewMutateAsyncMock.mockResolvedValueOnce({
      totalScanned: 1,
      imported: 1,
      skipped: 0,
      conflicts: 0,
      failed: 0,
      warnings: [],
    });

    const { result } = renderHook(() => useSessionSync());
    await act(async () => {
      await result.current.preview({
        targetProviderId: "claude",
        sourceProviderIds: ["codex"],
      });
    });

    expect(previewMutateAsyncMock).toHaveBeenCalledWith({
      targetProviderId: "claude",
      sourceProviderIds: ["codex"],
      dryRun: true,
    });
  });

  it("shows warning toast for preview-only sync results", async () => {
    syncMutateAsyncMock.mockResolvedValueOnce({
      totalScanned: 2,
      imported: 1,
      skipped: 0,
      conflicts: 1,
      failed: 0,
      warnings: ["sync execution is not implemented yet; preview only"],
    });

    const { result } = renderHook(() => useSessionSync());
    await act(async () => {
      await result.current.sync({
        targetProviderId: "claude",
        sourceProviderIds: ["codex"],
        dryRun: true,
      });
    });

    expect(toastWarningMock).toHaveBeenCalledTimes(1);
    expect(toastSuccessMock).not.toHaveBeenCalled();
  });
});
