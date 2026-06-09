import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import { CleverCoffeeProvider } from "@/context/CleverCoffeeContext.tsx";
import App from "./App.tsx";
import { ThemeProvider } from "./components/theme-provider";

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Root element #root not found");
}

createRoot(rootElement).render(
  <StrictMode>
    <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
      <CleverCoffeeProvider>
        <App />
      </CleverCoffeeProvider>
    </ThemeProvider>
  </StrictMode>,
);
