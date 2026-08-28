/**
 * 工具审批共享类型——与后端 `motis_chat::events::ApprovalRequestPayload`
 * 及 `motis_chat::approval_diff` 的序列化结构对齐。
 *
 * Motis 聊天与学术助手两个 composable、ApprovalDialog / ApprovalDiffModal
 * 组件共用，单一事实来源。
 */

/** diff 中的一行（后端 kind: "ctx" | "add" | "del"）。 */
export interface DiffLine {
  kind: 'ctx' | 'add' | 'del';
  text: string;
}

/** 单个变更块。 */
export interface DiffHunk {
  /** 本块之前被折叠的未变更行数。 */
  gap_before: number;
  lines: readonly DiffLine[];
}

/** 一次写操作的行级 diff（后端预演算；推演失败时不下发）。 */
export interface ApprovalDiff {
  path: string;
  added: number;
  removed: number;
  /** 行数超出上限，hunk 列表被截断。 */
  truncated: boolean;
  hunks: readonly DiffHunk[];
}

/** 后端审批请求事件 payload。 */
export interface ApprovalRequestPayload {
  id: string;
  tool_name: string;
  input: unknown;
  diff?: ApprovalDiff;
}

/** 待审批条目（弹窗与 composable 之间的共享形态）。 */
export interface PendingApproval {
  id: string;
  toolName: string;
  /** 输入参数（含 path / action / content 等）。 */
  input: Record<string, unknown>;
  /** 预演算 diff（无则弹窗回退截断摘要）。 */
  diff?: ApprovalDiff;
}
