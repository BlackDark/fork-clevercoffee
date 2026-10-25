import { mount } from "svelte";
import App from "./App.svelte";
import "./index.css";

const target = document.getElementById("app");
if (!target) throw new Error("Application root is missing");
mount(App, { target });
