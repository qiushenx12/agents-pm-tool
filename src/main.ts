import { createPinia } from "pinia";
import { createApp } from "vue";
import GridApp from "@/grid-app/GridApp.vue";
import { initTheme } from "@/shared/theme";
import "@/shared/theme.css";
import "@/shared/components.css";
import "@/grid-app/grid.css";

initTheme();
createApp(GridApp).use(createPinia()).mount("#app");
