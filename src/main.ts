import { createPinia } from "pinia";
import { createApp } from "vue";
import GridApp from "@/grid-app/GridApp.vue";
import "@/shared/theme.css";
import "@/shared/components.css";
import "@/grid-app/grid.css";

createApp(GridApp).use(createPinia()).mount("#app");
