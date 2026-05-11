import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  usePreviewSessionSyncMutation,
  useSyncSessionsToProviderMutation,
} from "@/lib/query";
import type { SessionSyncRequest, SessionSyncResult } from "@/lib/api/sessions";
import { extractErrorMessage } from "@/utils/errorUtils";

export function useSessionSync() {
  const { t } = useTranslation();
  const previewMutation = usePreviewSessionSyncMutation();
  const syncMutation = useSyncSessionsToProviderMutation();

  const preview = useCallback(
    async (request: SessionSyncRequest): Promise<SessionSyncResult> => {
      try {
        return await previewMutation.mutateAsync({ ...request, dryRun: true });
      } catch (error) {
        const detail = extractErrorMessage(error) || t("common.unknown");
        toast.error(
          t("sessionManager.syncPreviewFailed", {
            defaultValue: "预览会话同步失败: {{error}}",
            error: detail,
          }),
        );
        throw error;
      }
    },
    [previewMutation, t],
  );

  const sync = useCallback(
    async (request: SessionSyncRequest): Promise<SessionSyncResult> => {
      try {
        const result = await syncMutation.mutateAsync(request);
        const hasExecutionWarning = (result.warnings ?? []).some((warning) =>
          /not implemented|preview only|dryRun/i.test(warning),
        );
        if (hasExecutionWarning) {
          toast.warning(
            t("sessionManager.syncDryRunOnly", {
              defaultValue: "当前仅返回预览结果，尚未执行真实同步",
            }),
          );
        } else {
          toast.success(
            t("sessionManager.syncCompleted", {
              defaultValue: "会话同步执行完成",
            }),
          );
        }
        return result;
      } catch (error) {
        const detail = extractErrorMessage(error) || t("common.unknown");
        toast.error(
          t("sessionManager.syncFailed", {
            defaultValue: "会话同步失败: {{error}}",
            error: detail,
          }),
        );
        throw error;
      }
    },
    [syncMutation, t],
  );

  return {
    preview,
    sync,
    isPreviewing: previewMutation.isPending,
    isSyncing: syncMutation.isPending,
  };
}
