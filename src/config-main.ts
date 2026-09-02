import { createPinia } from "pinia";
import { createApp } from "vue";
import ConfigApp from "@/config-app/ConfigApp.vue";
import { initTheme } from "@/shared/theme";
import "@/shared/theme.css";
import "@/shared/components.css";

initTheme();
createApp(ConfigApp).use(createPinia()).mount("#app");
