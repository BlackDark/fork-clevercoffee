import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import App from "./App.tsx";
import { ThemeProvider } from "./components/theme-provider";
import { CleverCoffeeProvider } from "@/context/CleverCoffeeContext.tsx";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
      <CleverCoffeeProvider>
        <App />
      </CleverCoffeeProvider>
    </ThemeProvider>
  </StrictMode>
);
