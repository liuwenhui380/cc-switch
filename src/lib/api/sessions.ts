import { invoke } from "@tauri-apps/api/core";
import type { SessionMessage, SessionMeta } from "@/types";

export interface DeleteSessionOptions {
  providerId: string;
  sessionId: string;
  sourcePath: string;
}

export interface DeleteSessionResult extends DeleteSessionOptions {
  success: boolean;
  error?: string;
}

export type SessionSyncMode = "metadata_only" | "full_messages";

export type SessionSyncConflictPolicy =
  | "keep_target"
  | "overwrite"
  | "duplicate_new_id";

export interface SessionSyncRequest {
  targetProviderId: string;
  sourceProviderIds: string[];
  mode?: SessionSyncMode;
  conflictPolicy?: SessionSyncConflictPolicy;
  sinceTs?: number;
  dryRun?: boolean;
}

export interface SessionSyncResult {
  totalScanned: number;
  imported: number;
  skipped: number;
  conflicts: number;
  failed: number;
  warnings?: string[];
}

export interface SessionSyncCapabilities {
  executionSupported: boolean;
  reason?: string;
  supportedTargetProviders: string[];
}

export const sessionsApi = {
  async list(): Promise<SessionMeta[]> {
    return await invoke("list_sessions");
  },

  async getMessages(
    providerId: string,
    sourcePath: string,
  ): Promise<SessionMessage[]> {
    return await invoke("get_session_messages", { providerId, sourcePath });
  },

  async delete(options: DeleteSessionOptions): Promise<boolean> {
    const { providerId, sessionId, sourcePath } = options;
    return await invoke("delete_session", {
      providerId,
      sessionId,
      sourcePath,
    });
  },

  async deleteMany(
    items: DeleteSessionOptions[],
  ): Promise<DeleteSessionResult[]> {
    return await invoke("delete_sessions", { items });
  },

  async launchTerminal(options: {
    command: string;
    cwd?: string | null;
    customConfig?: string | null;
  }): Promise<boolean> {
    const { command, cwd, customConfig } = options;
    return await invoke("launch_session_terminal", {
      command,
      cwd,
      customConfig,
    });
  },

  async syncToProvider(
    request: SessionSyncRequest,
  ): Promise<SessionSyncResult> {
    return await invoke("sync_sessions_to_provider", { request });
  },

  async previewSync(request: SessionSyncRequest): Promise<SessionSyncResult> {
    return await invoke("preview_session_sync", {
      request: { ...request, dryRun: true },
    });
  },

  async getSyncCapabilities(
    targetProviderId?: string,
  ): Promise<SessionSyncCapabilities> {
    return await invoke("get_session_sync_capabilities", { targetProviderId });
  },
};
