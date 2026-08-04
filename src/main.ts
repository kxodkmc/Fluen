import { createApp } from "vue";
import App from "./App.vue";
import "./theme/styles/index.css";
import { i18nPlugin } from "./i18n";
import { startLogger } from "./utils/logger";

// 启动统一日志系统（定时 flush 前端日志到后端）
startLogger();

createApp(App).use(i18nPlugin).mount("#app");
