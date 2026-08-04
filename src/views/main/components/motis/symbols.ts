/**
 * Motis 模块 — 依赖注入键。
 *
 * useMotisChat 实例由 MainView 创建并通过 provide 注入，
 * MotisPanel 通过 inject 获取，确保整个组件树共享同一份对话状态。
 */
import type { InjectionKey } from 'vue';
import type { UseMotisChatReturn } from '../../composables/useMotisChat';

/** useMotisChat 实例注入键。 */
export const MOTIS_CHAT_KEY: InjectionKey<UseMotisChatReturn> = Symbol('motis-chat');
