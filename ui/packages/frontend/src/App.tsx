import { BrowserRouter, Route, Routes } from "react-router-dom";
import { basePath } from "@/lib/config";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Layout } from "./components/Layout";
import { Toaster } from "./components/ui/sonner";
import { AboutPage } from "./pages/AboutPage";
import { ConfigPage } from "./pages/ConfigPage";
import { HomePage } from "./pages/HomePage";
import { NotFoundPage } from "./pages/NotFoundPage";
import { SystemPage } from "./pages/SystemPage";

function App() {
  return (
    <ErrorBoundary>
      <BrowserRouter
        basename={basePath === "/" ? undefined : basePath.replace(/\/$/, "")}
      >
        <Routes>
          <Route path="/" element={<Layout />}>
            <Route index element={<HomePage />} />
            <Route path="config/:filter" element={<ConfigPage />} />
            <Route path="system" element={<SystemPage />} />
            <Route path="about" element={<AboutPage />} />
            {/* Catch-all route for 404 pages */}
            <Route path="*" element={<NotFoundPage />} />
          </Route>
        </Routes>
        <Toaster position="bottom-right" />
      </BrowserRouter>
    </ErrorBoundary>
  );
}

export default App;
