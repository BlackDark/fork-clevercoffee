import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import {
  BrowserRouter,
  Link,
  Outlet,
  Route,
  Routes,
  useLocation,
  useParams,
} from "./router";

(
  globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

function Filter() {
  const { filter } = useParams<{ filter: string }>();
  return <p>filter:{filter}</p>;
}

function Shell() {
  const location = useLocation();
  return (
    <div>
      <span>path:{location.pathname}</span>
      <Link to="/system">System</Link>
      <Link to="/config/behavior" onClick={(event) => event.preventDefault()}>
        Cancelled
      </Link>
      <Link to="/about" target="_blank">
        External
      </Link>
      <Outlet />
    </div>
  );
}

function AppRoutes() {
  return (
    <BrowserRouter basename="/ui">
      <Routes>
        <Route path="/" element={<Shell />}>
          <Route index element={<p>home</p>} />
          <Route path="config/:filter" element={<Filter />} />
          <Route path="system" element={<p>system</p>} />
          <Route path="*" element={<p>missing</p>} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}

describe("router", () => {
  let root: Root | null = null;
  let host: HTMLDivElement | null = null;

  afterEach(() => {
    act(() => {
      root?.unmount();
    });
    host?.remove();
    root = null;
    host = null;
    window.history.replaceState(null, "", "/");
  });

  function renderAt(url: string) {
    window.history.replaceState(null, "", url);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => {
      root?.render(<AppRoutes />);
    });
    return host;
  }

  function click(link: HTMLAnchorElement, init?: MouseEventInit) {
    act(() => {
      link.dispatchEvent(
        new MouseEvent("click", {
          bubbles: true,
          cancelable: true,
          button: 0,
          ...init,
        }),
      );
    });
  }

  it("serves the index route under the basename", () => {
    const view = renderAt("/ui");
    expect(view.textContent).toContain("path:/");
    expect(view.textContent).toContain("home");
  });

  it("reads a path param and ignores a trailing slash", () => {
    const view = renderAt("/ui/config/behavior/");
    expect(view.textContent).toContain("path:/config/behavior");
    expect(view.textContent).toContain("filter:behavior");
  });

  it("navigates on a plain click and leaves modified clicks alone", () => {
    const view = renderAt("/ui/");
    const system = [...view.querySelectorAll("a")].find(
      (anchor) => anchor.textContent === "System",
    );
    expect(system?.getAttribute("href")).toBe("/ui/system");
    if (!system) {
      throw new Error("missing system link");
    }
    click(system, { ctrlKey: true });
    expect(view.textContent).toContain("home");
    click(system);
    expect(view.textContent).toContain("path:/system");
    expect(view.textContent).toContain("system");
    expect(window.location.pathname).toBe("/ui/system");
  });

  it("does not navigate when the click handler cancels the event", () => {
    const view = renderAt("/ui/");
    const cancelled = [...view.querySelectorAll("a")].find(
      (anchor) => anchor.textContent === "Cancelled",
    );
    if (!cancelled) {
      throw new Error("missing cancelled link");
    }
    click(cancelled);
    expect(view.textContent).toContain("home");
  });

  it("renders the splat inside the layout", () => {
    const view = renderAt("/ui/nope");
    expect(view.textContent).toContain("path:/nope");
    expect(view.textContent).toContain("missing");
  });

  it("rereads the address bar on popstate", () => {
    const view = renderAt("/ui/");
    const system = [...view.querySelectorAll("a")].find(
      (anchor) => anchor.textContent === "System",
    );
    if (!system) {
      throw new Error("missing system link");
    }
    click(system);
    act(() => {
      window.history.pushState(null, "", "/ui/");
      window.dispatchEvent(new PopStateEvent("popstate"));
    });
    expect(view.textContent).toContain("path:/");
    expect(view.textContent).toContain("home");
  });
});
