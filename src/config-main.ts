import { createPinia } from "pinia";
import { createApp } from "vue";
import ConfigApp from "@/config-app/ConfigApp.vue";
import "@/shared/theme.css";
import "@/shared/components.css";

createApp(ConfigApp).use(createPinia()).mount("#app");
