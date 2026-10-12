// Five routes do not need react-router. Its published exports resolve to the
// development build, and after minify that was still 38 KB of the JS bundle.
// This covers the calls the UI actually makes: `BrowserRouter`, `Routes`,
// `Route`, `Link`, `Outlet`, `useLocation`, `useParams`.

import {
  Children,
  createContext,
  isValidElement,
  type MouseEvent,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

type Location = {
  pathname: string;
  search: string;
  hash: string;
};

type Params = Record<string, string>;

type Match = {
  params: Params;
  element: ReactNode;
  child: Match | null;
};

type RouteNode = {
  path?: string;
  index?: boolean;
  element: ReactNode;
  children?: RouteNode[];
};

type RouterValue = {
  basename: string;
  location: Location;
  navigate: (to: string) => void;
};

const RouterContext = createContext<RouterValue | null>(null);
const MatchContext = createContext<Match | null>(null);

function useRouter(): RouterValue {
  const value = useContext(RouterContext);
  if (!value) {
    throw new Error("router hooks must be used inside BrowserRouter");
  }
  return value;
}

export function useLocation(): Location {
  return useRouter().location;
}

export function useParams<
  T extends Record<string, string | undefined> = Record<string, string>,
>(): T {
  const match = useContext(MatchContext);
  return (match?.params ?? {}) as T;
}

function splitPath(path: string): string[] {
  const noHash = path.split("#")[0] ?? "";
  const noQuery = noHash.split("?")[0] ?? "";
  const trimmed = noQuery.length > 1 ? noQuery.replace(/\/+$/, "") : noQuery;
  return trimmed.split("/").filter((segment) => segment.length > 0);
}

function decode(segment: string): string {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}

function matchExact(pattern: string[], segments: string[]): Params | null {
  if (pattern.length !== segments.length) {
    return null;
  }
  const params: Params = {};
  for (let i = 0; i < pattern.length; i += 1) {
    const part = pattern[i] ?? "";
    const segment = segments[i] ?? "";
    if (part.startsWith(":")) {
      params[part.slice(1)] = decode(segment);
    } else if (part !== segment) {
      return null;
    }
  }
  return params;
}

function rankPattern(pattern: string[]): number {
  let rank = 0;
  for (const part of pattern) {
    rank += part.startsWith(":") ? 10 : 100;
  }
  return rank;
}

function matchList(
  segments: string[],
  routes: RouteNode[],
  inherited: Params,
): Match | null {
  let best: { rank: number; match: Match } | null = null;
  for (const route of routes) {
    const found = matchRoute(segments, route, inherited);
    if (!found) {
      continue;
    }
    if (!best || found.rank > best.rank) {
      best = found;
    }
  }
  return best?.match ?? null;
}

function matchRoute(
  segments: string[],
  route: RouteNode,
  inherited: Params,
): { rank: number; match: Match } | null {
  if (route.index) {
    if (segments.length !== 0) {
      return null;
    }
    return {
      rank: 1_000,
      match: { params: { ...inherited }, element: route.element, child: null },
    };
  }
  if (route.path === "*") {
    return {
      rank: 0,
      match: {
        params: { ...inherited, "*": segments.join("/") },
        element: route.element,
        child: null,
      },
    };
  }
  const pattern = splitPath(route.path ?? "/");
  if (route.children && route.children.length > 0) {
    if (pattern.length > segments.length) {
      return null;
    }
    const own = matchExact(pattern, segments.slice(0, pattern.length));
    if (!own) {
      return null;
    }
    const params = { ...inherited, ...own };
    const child = matchList(
      segments.slice(pattern.length),
      route.children,
      params,
    );
    if (!child) {
      return null;
    }
    return {
      rank: rankPattern(pattern) + childRank(child) + 1,
      match: { params: child.params, element: route.element, child },
    };
  }
  const own = matchExact(pattern, segments);
  if (!own) {
    return null;
  }
  return {
    rank: rankPattern(pattern) + 10,
    match: {
      params: { ...inherited, ...own },
      element: route.element,
      child: null,
    },
  };
}

function childRank(match: Match): number {
  if (match.child) {
    return 1 + childRank(match.child);
  }
  return match.params["*"] === undefined ? 10 : 0;
}

function stripBasename(pathname: string, basename: string): string {
  const base = basename.replace(/\/$/, "");
  if (!base) {
    return pathname;
  }
  if (pathname === base) {
    return "/";
  }
  if (pathname.startsWith(`${base}/`)) {
    return pathname.slice(base.length);
  }
  return pathname;
}

function normalizePath(pathname: string): string {
  if (!pathname.startsWith("/")) {
    return normalizePath(`/${pathname}`);
  }
  if (pathname.length > 1 && pathname.endsWith("/")) {
    return pathname.replace(/\/+$/, "");
  }
  return pathname;
}

function joinBasename(basename: string, pathname: string): string {
  const base = basename.replace(/\/$/, "");
  if (!base) {
    return pathname;
  }
  if (pathname === "/") {
    return base;
  }
  return `${base}${pathname}`;
}

function readLocation(basename: string): Location {
  const { pathname, search, hash } = window.location;
  return {
    pathname: normalizePath(stripBasename(pathname, basename)),
    search,
    hash,
  };
}

function toHref(basename: string, to: string): string {
  const url = new URL(to, "http://router.local");
  return (
    joinBasename(basename, normalizePath(url.pathname)) + url.search + url.hash
  );
}

export function BrowserRouter({
  basename = "",
  children,
}: {
  basename?: string;
  children: ReactNode;
}) {
  const [location, setLocation] = useState(() => readLocation(basename));

  useEffect(() => {
    setLocation(readLocation(basename));
    const onPop = () => setLocation(readLocation(basename));
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, [basename]);

  const navigate = useCallback(
    (to: string) => {
      const url = new URL(to, "http://router.local");
      const pathname = normalizePath(url.pathname);
      window.history.pushState(
        null,
        "",
        joinBasename(basename, pathname) + url.search + url.hash,
      );
      setLocation({ pathname, search: url.search, hash: url.hash });
    },
    [basename],
  );

  const value = useMemo(
    () => ({ basename, location, navigate }),
    [basename, location, navigate],
  );

  return (
    <RouterContext.Provider value={value}>{children}</RouterContext.Provider>
  );
}

export type RouteProps = {
  path?: string;
  index?: boolean;
  element?: ReactNode;
  children?: ReactNode;
};

export function Route(_props: RouteProps) {
  return null;
}

function toNodes(children: ReactNode): RouteNode[] {
  const nodes: RouteNode[] = [];
  Children.forEach(children, (child) => {
    if (!isValidElement(child) || child.type !== Route) {
      return;
    }
    const props = child.props as RouteProps;
    nodes.push({
      path: props.path,
      index: props.index,
      element: props.element ?? null,
      children: props.children ? toNodes(props.children) : undefined,
    });
  });
  return nodes;
}

export function Routes({ children }: { children: ReactNode }) {
  const { pathname } = useLocation();
  const match = matchList(splitPath(pathname), toNodes(children), {});
  if (!match) {
    return null;
  }
  return (
    <MatchContext.Provider value={match}>{match.element}</MatchContext.Provider>
  );
}

export function Outlet() {
  const match = useContext(MatchContext);
  if (!match?.child) {
    return null;
  }
  return (
    <MatchContext.Provider value={match.child}>
      {match.child.element}
    </MatchContext.Provider>
  );
}

type LinkProps = Omit<React.ComponentProps<"a">, "href"> & {
  to: string;
};

function isPlainLeftClick(event: MouseEvent<HTMLAnchorElement>): boolean {
  return (
    event.button === 0 &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.shiftKey &&
    !event.altKey
  );
}

export function Link({ to, onClick, target, ...rest }: LinkProps) {
  const { basename, navigate } = useRouter();
  return (
    <a
      {...rest}
      href={toHref(basename, to)}
      target={target}
      onClick={(event) => {
        onClick?.(event);
        if (event.defaultPrevented) {
          return;
        }
        if (!isPlainLeftClick(event)) {
          return;
        }
        if (target && target !== "_self") {
          return;
        }
        if (event.currentTarget.hasAttribute("download")) {
          return;
        }
        event.preventDefault();
        navigate(to);
      }}
    />
  );
}
