import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Layout } from "./components/Layout";
import { HomePage } from "./pages/HomePage";
import { ConfigPage } from "./pages/ConfigPage";
import { SystemPage } from "./pages/SystemPage";
import { AboutPage } from "./pages/AboutPage";
import { NotFoundPage } from "./pages/NotFoundPage";
import { Toaster } from "./components/ui/sonner";
import { ErrorBoundary } from "./components/ErrorBoundary";

// Get base path from environment variable or use Vite's base
const basePath =
  import.meta.env.VITE_BASE_PATH || import.meta.env.BASE_URL || "/";

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
        <Toaster />
      </BrowserRouter>
    </ErrorBoundary>
  );
}

export default App;
