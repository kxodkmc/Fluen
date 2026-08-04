/**
 * 国际化模块 — useI18n composable。
 *
 * 包装 vue-i18n 的 useI18n，暴露：
 *   - t：翻译函数（global scope）
 *   - locale：当前语言（WritableComputedRef<Language>）
 *   - setLocale：切换语言，可选持久化到 AppConfig
 *   - availableLocales：受支持的 locale 列表
 *
 * 设计要点：
 *   - 为避免循环依赖（i18n ↔ useAppConfig），useAppConfig 采用动态 import，
 *     仅在 setLocale 持久化时调用。
 *   - locale 通过 computed 包装 i18nInstance.global.locale，保证类型为 Language。
 */

import { computed, type WritableComputedRef } from 'vue';
import { useI18n as useVueI18n } from 'vue-i18n';
import { i18nInstance } from '../index';
import { SUPPORTED_LOCALES } from '../locales';
import type { Language } from '../types';

/** setLocale 选项。 */
export interface SetLocaleOptions {
  /** 是否持久化到 AppConfig，默认 true。 */
  persist?: boolean;
}

export interface UseI18nReturn {
  /** 翻译函数（global scope）。 */
  t: ReturnType<typeof useVueI18n>['t'];
  /** 当前语言（响应式，可读写）。 */
  locale: WritableComputedRef<Language>;
  /** 切换语言。 */
  setLocale: (locale: Language, options?: SetLocaleOptions) => Promise<void>;
  /** 受支持的 locale 列表。 */
  availableLocales: typeof SUPPORTED_LOCALES;
}

export function useI18n(): UseI18nReturn {
  const { t } = useVueI18n({ useScope: 'global' });

  const locale: WritableComputedRef<Language> = computed({
    get: () => i18nInstance.global.locale.value as Language,
    set: (val: Language) => {
      i18nInstance.global.locale.value = val;
    },
  });

  /**
   * 切换语言。
   *
   * persist=true（默认）时同步 i18n 实例并调用 useAppConfig().updateLanguage 持久化。
   * persist=false 时仅同步 i18n 实例（用于初始化避免循环写入）。
   */
  async function setLocale(lang: Language, options?: SetLocaleOptions): Promise<void> {
    const shouldPersist = options?.persist ?? true;
    // 同步 i18n 实例
    i18nInstance.global.locale.value = lang;
    if (shouldPersist) {
      // 动态 import 避免与 useAppConfig 形成循环依赖
      const { useAppConfig } = await import('../../composables/useAppConfig');
      const { updateLanguage } = useAppConfig();
      await updateLanguage(lang);
    }
  }

  const availableLocales = SUPPORTED_LOCALES;

  return { t, locale, setLocale, availableLocales };
}
