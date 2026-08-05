<script setup lang="ts">
/**
 * RecentProjects — 欢迎页（无项目打开时的主内容区）。
 *
 * 设计语言遵循 DESIGN.md（MiniMax 风格）：
 *   - 黑白主色调，黑色 pill 主按钮 + 描边次按钮
 *   - 白色卡片 16px 圆角 + 1px hairline 边框
 *   - DM Sans 字体栈，标题 -0.5px 字距
 *   - 扁平 + 边框，仅在 hover 时切换边框色
 *
 * 布局：
 *   - 顶部品牌区（Logo + 标题 + 副标题）
 *   - 双 CTA（新建文章 / 打开文章）
 *   - 最近项目卡片网格（点击即打开对应项目）
 *
 * 数量受 `AppConfig.recent_projects_count` 控制（默认 4，范围 1-8）。
 * 「新建文章」「打开文章」通过事件向上冒泡，由 MainView 处理具体对话框与文件选择。
 */
import { onMounted } from 'vue';
import { useI18n } from '../../../../i18n';
import { useRecentProjects } from '../../../../composables/useRecentProjects';
import { useProject } from '../../../../composables/useProject';

const { t } = useI18n();
const { entries, refresh } = useRecentProjects();
const { openProject, isLoading } = useProject();

const emit = defineEmits<{
  (e: 'new-article'): void;
  (e: 'open-article'): void;
}>();

onMounted(() => {
  void refresh(true);
});

/** 点击项目卡片：调用 openProject 打开。 */
async function handleOpen(path: string): Promise<void> {
  if (isLoading.value) return;
  await openProject(path);
}

/**
 * 格式化打开时间为相对时间文案。
 *
 * 输出形如「刚刚」「3 分钟前」「2 小时前」「昨天」「3 天前」或绝对日期。
 * 解析失败时回退为原始字符串。
 */
function formatRelativeTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;

  const now = Date.now();
  const diffMs = now - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHour = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHour / 24);

  if (diffSec < 60) return t('main.content.welcome.recent.justNow');
  if (diffMin < 60) return t('main.content.welcome.recent.minutesAgo', { count: diffMin });
  if (diffHour < 24) return t('main.content.welcome.recent.hoursAgo', { count: diffHour });
  if (diffDay === 1) return t('main.content.welcome.recent.yesterday');
  if (diffDay < 7) return t('main.content.welcome.recent.daysAgo', { count: diffDay });

  // 超过 7 天显示绝对日期（YYYY-MM-DD）
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, '0');
  const d = String(date.getDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}
</script>

<template>
  <div class="welcome">
    <!-- ── 品牌区 ───────────────────────────────────────────────────── -->
    <header class="welcome__brand">
      <div class="welcome__mark">
        <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <path d="M4 4h12v16H4V4z" />
          <path d="M18 8v12a2 2 0 0 1-2 2" />
          <path d="M8 8h4M8 12h4M8 16h2" />
        </svg>
      </div>
      <h1 class="welcome__title">{{ t('main.content.welcome.title') }}</h1>
      <p class="welcome__subtitle">{{ t('main.content.welcome.subtitle') }}</p>
    </header>

    <!-- ── 主操作 CTA ───────────────────────────────────────────────── -->
    <div class="welcome__actions">
      <button class="btn btn--primary" type="button" @click="emit('new-article')">
        {{ t('main.titleBar.menus.filesItems.newArticle') }}
      </button>
      <button class="btn btn--secondary" type="button" @click="emit('open-article')">
        {{ t('main.titleBar.menus.filesItems.openArticle') }}
      </button>
    </div>

    <!-- ── 最近打开 ─────────────────────────────────────────────────── -->
    <section v-if="entries.length > 0" class="recent">
      <h2 class="recent__label">{{ t('main.content.welcome.recent.title') }}</h2>
      <div class="recent__grid">
        <button
          v-for="entry in entries"
          :key="entry.project_path"
          class="recent-card"
          type="button"
          :disabled="isLoading"
          @click="handleOpen(entry.project_path)"
        >
          <span class="recent-card__title" :title="entry.title">{{ entry.title }}</span>
          <span class="recent-card__meta">
            <span class="recent-card__author" :title="entry.author">{{ entry.author }}</span>
            <span class="recent-card__sep">·</span>
            <span class="recent-card__time">{{ formatRelativeTime(entry.opened_at) }}</span>
          </span>
        </button>
      </div>
    </section>

    <!-- ── 空状态提示 ───────────────────────────────────────────────── -->
    <p v-else class="welcome__hint">{{ t('main.content.welcome.hint') }}</p>
  </div>
</template>

<style scoped>
/* ╔═ 容器 ═════════════════════════════════════════════════════════════ */
.welcome {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 32px;
  padding: 48px 32px;
  background: var(--fluen-canvas);
}

/* ╔═ 品牌区 ═══════════════════════════════════════════════════════════ */
.welcome__brand {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}

.welcome__mark {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 56px;
  height: 56px;
  border-radius: 16px;
  background: var(--fluen-primary);
  color: var(--fluen-on-primary);
  margin-bottom: 4px;
}

.welcome__title {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 32px;
  font-weight: 600;
  line-height: 1.25;
  letter-spacing: -0.5px;
  color: var(--fluen-ink);
}

.welcome__subtitle {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  font-weight: 400;
  line-height: 1.5;
  color: var(--fluen-steel);
}

/* ╔═ CTA 按钮 ═════════════════════════════════════════════════════════ */
.welcome__actions {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
}

.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 11px 24px;
  border-radius: 9999px;
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  font-weight: 600;
  line-height: 1.4;
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
}

.btn--primary {
  background: var(--fluen-primary);
  color: var(--fluen-on-primary);
  border: 1px solid var(--fluen-primary);
}

.btn--primary:hover {
  background: var(--fluen-charcoal);
  border-color: var(--fluen-charcoal);
}

.btn--primary:active {
  background: var(--fluen-ink-strong);
}

.btn--secondary {
  background: transparent;
  color: var(--fluen-ink);
  border: 1px solid var(--fluen-ink);
}

.btn--secondary:hover {
  background: var(--fluen-hover);
}

/* ╔═ 最近项目 ═════════════════════════════════════════════════════════ */
.recent {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  width: 100%;
  max-width: 720px;
}

.recent__label {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  font-weight: 600;
  line-height: 1.5;
  color: var(--fluen-stone);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.recent__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 12px;
  width: 100%;
}

/* ── 项目卡片 ───────────────────────────────────────────────────────── */
.recent-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 20px;
  background: var(--fluen-canvas);
  border: 1px solid var(--fluen-hairline);
  border-radius: 16px;
  cursor: pointer;
  text-align: left;
  font-family: var(--fluen-font-sans);
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}

.recent-card:hover:not(:disabled) {
  border-color: var(--fluen-accent);
  box-shadow: var(--fluen-shadow-card);
}

.recent-card:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.recent-card__title {
  font-size: 16px;
  font-weight: 700;
  line-height: 1.4;
  color: var(--fluen-ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.recent-card__meta {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 400;
  line-height: 1.5;
  color: var(--fluen-stone);
  white-space: nowrap;
  overflow: hidden;
}

.recent-card__author {
  overflow: hidden;
  text-overflow: ellipsis;
}

.recent-card__sep {
  flex-shrink: 0;
  opacity: 0.6;
}

.recent-card__time {
  flex-shrink: 0;
}

/* ╔═ 空状态 ─═════════════════════════════════════════════════════════ */
.welcome__hint {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  color: var(--fluen-stone);
}

/* ╔═ 响应式 ═══════════════════════════════════════════════════════════ */
@media (max-width: 640px) {
  .recent__grid {
    grid-template-columns: 1fr;
  }

  .welcome__actions {
    flex-direction: column;
    width: 100%;
    max-width: 280px;
  }

  .btn {
    width: 100%;
  }
}
</style>
