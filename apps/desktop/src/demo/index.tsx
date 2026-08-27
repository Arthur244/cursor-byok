import { installDemoApi } from "./api";

installDemoApi();
document.documentElement.dataset.platform = "macos";
localStorage.setItem("cursor-byok.locale", "zh-CN");
localStorage.setItem("cursor-byok.theme", "default-dark");

void import("../index");
