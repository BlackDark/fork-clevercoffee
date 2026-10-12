// Icon geometry from Lucide 1.52.0 (https://lucide.dev).
//
// ISC License
// Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as part of Feather (MIT).
// All other copyright (c) for Lucide are held by Lucide Contributors 2022.
//
// Permission to use, copy, modify, and/or distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR
// IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
//
// Inlined so the lucide-react runtime (case conversion, the shared icon
// factory) is not embedded in the firmware image. Only the marks this UI
// draws are here.

import { createElement, type ReactNode, type SVGProps } from "react";

type IconNode = readonly [string, Record<string, string>];

export type IconProps = SVGProps<SVGSVGElement> & {
  size?: number | string;
};

function hasA11yProp(props: SVGProps<SVGSVGElement>): boolean {
  for (const prop of Object.keys(props)) {
    if (prop.startsWith("aria-") || prop === "role" || prop === "title") {
      return true;
    }
  }
  return false;
}

function createIcon(name: string, nodes: readonly IconNode[]) {
  function Icon({
    className,
    size = 24,
    children,
    ...rest
  }: IconProps): ReactNode {
    const decorative = !children && !hasA11yProp(rest);
    return (
      // Decorative marks. A name is passed in from the caller (aria-label,
      // title, or a child) when the icon is the only content of a control.
      // biome-ignore lint/a11y/noSvgWithoutTitle: aria-hidden when decorative
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden={decorative ? true : undefined}
        className={["lucide", `lucide-${name}`, className]
          .filter(Boolean)
          .join(" ")}
        {...rest}
      >
        {nodes.map(([tag, attrs]) => {
          const { key, ...shape } = attrs;
          return createElement(tag, { key, ...shape });
        })}
        {children}
      </svg>
    );
  }
  return Icon;
}

const ActivityNodes = [
  [
    "path",
    {
      d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
      key: "169zse",
    },
  ],
] as const satisfies readonly IconNode[];
export const Activity = createIcon("activity", ActivityNodes);

const CircleAlertNodes = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
  ["line", { x1: "12", x2: "12", y1: "8", y2: "12", key: "1pkeuh" }],
  ["line", { x1: "12", x2: "12.01", y1: "16", y2: "16", key: "4dfq90" }],
] as const satisfies readonly IconNode[];
export const CircleAlert = createIcon("circle-alert", CircleAlertNodes);

const TriangleAlertNodes = [
  [
    "path",
    {
      d: "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3",
      key: "wmoenq",
    },
  ],
  ["path", { d: "M12 9v4", key: "juzpu7" }],
  ["path", { d: "M12 17h.01", key: "p32p05" }],
] as const satisfies readonly IconNode[];
export const TriangleAlert = createIcon("triangle-alert", TriangleAlertNodes);

const ArrowLeftNodes = [
  ["path", { d: "m12 19-7-7 7-7", key: "1l729n" }],
  ["path", { d: "M19 12H5", key: "x3x0zl" }],
] as const satisfies readonly IconNode[];
export const ArrowLeft = createIcon("arrow-left", ArrowLeftNodes);

const CircleCheckBigNodes = [
  ["path", { d: "M21.801 10A10 10 0 1 1 17 3.335", key: "yps3ct" }],
  ["path", { d: "m9 11 3 3L22 4", key: "1pflzl" }],
] as const satisfies readonly IconNode[];
export const CircleCheckBig = createIcon(
  "circle-check-big",
  CircleCheckBigNodes,
);

const CheckNodes = [
  ["path", { d: "M20 6 9 17l-5-5", key: "1gmf2c" }],
] as const satisfies readonly IconNode[];
export const Check = createIcon("check", CheckNodes);

const ChevronDownNodes = [
  ["path", { d: "m6 9 6 6 6-6", key: "qrunsl" }],
] as const satisfies readonly IconNode[];
export const ChevronDown = createIcon("chevron-down", ChevronDownNodes);

const ChevronRightNodes = [
  ["path", { d: "m9 18 6-6-6-6", key: "mthhwq" }],
] as const satisfies readonly IconNode[];
export const ChevronRight = createIcon("chevron-right", ChevronRightNodes);

const ChevronUpNodes = [
  ["path", { d: "m18 15-6-6-6 6", key: "153udz" }],
] as const satisfies readonly IconNode[];
export const ChevronUp = createIcon("chevron-up", ChevronUpNodes);

const CircleNodes = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
] as const satisfies readonly IconNode[];
export const Circle = createIcon("circle", CircleNodes);

const DownloadNodes = [
  ["path", { d: "M12 15V3", key: "m9g1x1" }],
  ["path", { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", key: "ih7n3h" }],
  ["path", { d: "m7 10 5 5 5-5", key: "brsn70" }],
] as const satisfies readonly IconNode[];
export const Download = createIcon("download", DownloadNodes);

const DropletsNodes = [
  [
    "path",
    {
      d: "M7 16.3c2.2 0 4-1.83 4-4.05 0-1.16-.57-2.26-1.71-3.19S7.29 6.75 7 5.3c-.29 1.45-1.14 2.84-2.29 3.76S3 11.1 3 12.25c0 2.22 1.8 4.05 4 4.05z",
      key: "1ptgy4",
    },
  ],
  [
    "path",
    {
      d: "M12.56 6.6A10.97 10.97 0 0 0 14 3.02c.5 2.5 2 4.9 4 6.5s3 3.5 3 5.5a6.98 6.98 0 0 1-11.91 4.97",
      key: "1sl1rz",
    },
  ],
] as const satisfies readonly IconNode[];
export const Droplets = createIcon("droplets", DropletsNodes);

const GlobeNodes = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
  [
    "path",
    { d: "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20", key: "13o1zl" },
  ],
  ["path", { d: "M2 12h20", key: "9i4pu4" }],
] as const satisfies readonly IconNode[];
export const Globe = createIcon("globe", GlobeNodes);

const HardDriveNodes = [
  ["path", { d: "M10 16h.01", key: "1bzywj" }],
  [
    "path",
    {
      d: "M2.212 11.577a2 2 0 0 0-.212.896V18a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-5.527a2 2 0 0 0-.212-.896L18.55 5.11A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
      key: "18tbho",
    },
  ],
  ["path", { d: "M21.946 12.013H2.054", key: "zqlbp7" }],
  ["path", { d: "M6 16h.01", key: "1pmjb7" }],
] as const satisfies readonly IconNode[];
export const HardDrive = createIcon("hard-drive", HardDriveNodes);

const CircleQuestionMarkNodes = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
  ["path", { d: "M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3", key: "1u773s" }],
  ["path", { d: "M12 17h.01", key: "p32p05" }],
] as const satisfies readonly IconNode[];
export const CircleQuestionMark = createIcon(
  "circle-question-mark",
  CircleQuestionMarkNodes,
);

const HouseNodes = [
  ["path", { d: "M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8", key: "5wwlr5" }],
  [
    "path",
    {
      d: "M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
      key: "r6nss1",
    },
  ],
] as const satisfies readonly IconNode[];
export const House = createIcon("house", HouseNodes);

const InfoNodes = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
  ["path", { d: "M12 16v-4", key: "1dtifu" }],
  ["path", { d: "M12 8h.01", key: "e9boi3" }],
] as const satisfies readonly IconNode[];
export const Info = createIcon("info", InfoNodes);

const LinkNodes = [
  [
    "path",
    {
      d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71",
      key: "1cjeqo",
    },
  ],
  [
    "path",
    {
      d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
      key: "19qd67",
    },
  ],
] as const satisfies readonly IconNode[];
export const Link = createIcon("link", LinkNodes);

const Link2Nodes = [
  ["path", { d: "M9 17H7A5 5 0 0 1 7 7h2", key: "8i5ue5" }],
  ["path", { d: "M15 7h2a5 5 0 1 1 0 10h-2", key: "1b9ql8" }],
  ["line", { x1: "8", x2: "16", y1: "12", y2: "12", key: "1jonct" }],
] as const satisfies readonly IconNode[];
export const Link2 = createIcon("link-2", Link2Nodes);

const ListRestartNodes = [
  ["path", { d: "M21 5H3", key: "1fi0y6" }],
  ["path", { d: "M7 12H3", key: "13ou7f" }],
  ["path", { d: "M7 19H3", key: "wbqt3n" }],
  [
    "path",
    {
      d: "M12 18a5 5 0 0 0 9-3 4.5 4.5 0 0 0-4.5-4.5c-1.33 0-2.54.54-3.41 1.41L11 14",
      key: "qth677",
    },
  ],
  ["path", { d: "M11 10v4h4", key: "172dkj" }],
] as const satisfies readonly IconNode[];
export const ListRestart = createIcon("list-restart", ListRestartNodes);

const LoaderCircleNodes = [
  ["path", { d: "M21 12a9 9 0 1 1-6.219-8.56", key: "13zald" }],
] as const satisfies readonly IconNode[];
export const LoaderCircle = createIcon("loader-circle", LoaderCircleNodes);

const MenuNodes = [
  ["path", { d: "M4 5h16", key: "1tepv9" }],
  ["path", { d: "M4 12h16", key: "1lakjw" }],
  ["path", { d: "M4 19h16", key: "1djgab" }],
] as const satisfies readonly IconNode[];
export const Menu = createIcon("menu", MenuNodes);

const MessageCircleNodes = [
  [
    "path",
    {
      d: "M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719",
      key: "1sd12s",
    },
  ],
] as const satisfies readonly IconNode[];
export const MessageCircle = createIcon("message-circle", MessageCircleNodes);

const MoonNodes = [
  [
    "path",
    {
      d: "M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401",
      key: "kfwtm",
    },
  ],
] as const satisfies readonly IconNode[];
export const Moon = createIcon("moon", MoonNodes);

const PowerNodes = [
  ["path", { d: "M12 2v10", key: "mnfbl" }],
  ["path", { d: "M18.4 6.6a9 9 0 1 1-12.77.04", key: "obofu9" }],
] as const satisfies readonly IconNode[];
export const Power = createIcon("power", PowerNodes);

const RefreshCcwNodes = [
  [
    "path",
    { d: "M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8", key: "14sxne" },
  ],
  ["path", { d: "M3 3v5h5", key: "1xhq8a" }],
  [
    "path",
    { d: "M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16", key: "1hlbsb" },
  ],
  ["path", { d: "M16 16h5v5", key: "ccwih5" }],
] as const satisfies readonly IconNode[];
export const RefreshCcw = createIcon("refresh-ccw", RefreshCcwNodes);

const RefreshCwNodes = [
  [
    "path",
    { d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8", key: "v9h5vc" },
  ],
  ["path", { d: "M21 3v5h-5", key: "1q7to0" }],
  [
    "path",
    { d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16", key: "3uifl3" },
  ],
  ["path", { d: "M8 16H3v5", key: "1cv678" }],
] as const satisfies readonly IconNode[];
export const RefreshCw = createIcon("refresh-cw", RefreshCwNodes);

const RotateCcwNodes = [
  [
    "path",
    { d: "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8", key: "1357e3" },
  ],
  ["path", { d: "M3 3v5h5", key: "1xhq8a" }],
] as const satisfies readonly IconNode[];
export const RotateCcw = createIcon("rotate-ccw", RotateCcwNodes);

const SaveNodes = [
  [
    "path",
    {
      d: "M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z",
      key: "1c8476",
    },
  ],
  ["path", { d: "M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7", key: "1ydtos" }],
  ["path", { d: "M7 3v4a1 1 0 0 0 1 1h7", key: "t51u73" }],
] as const satisfies readonly IconNode[];
export const Save = createIcon("save", SaveNodes);

const SettingsNodes = [
  [
    "path",
    {
      d: "M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915",
      key: "1i5ecw",
    },
  ],
  ["circle", { cx: "12", cy: "12", r: "3", key: "1v7zrd" }],
] as const satisfies readonly IconNode[];
export const Settings = createIcon("settings", SettingsNodes);

const SunNodes = [
  ["circle", { cx: "12", cy: "12", r: "4", key: "4exip2" }],
  ["path", { d: "M12 2v2", key: "tus03m" }],
  ["path", { d: "M12 20v2", key: "1lh1kg" }],
  ["path", { d: "m4.93 4.93 1.41 1.41", key: "149t6j" }],
  ["path", { d: "m17.66 17.66 1.41 1.41", key: "ptbguv" }],
  ["path", { d: "M2 12h2", key: "1t8f8n" }],
  ["path", { d: "M20 12h2", key: "1q8mjw" }],
  ["path", { d: "m6.34 17.66-1.41 1.41", key: "1m8zz5" }],
  ["path", { d: "m19.07 4.93-1.41 1.41", key: "1shlcs" }],
] as const satisfies readonly IconNode[];
export const Sun = createIcon("sun", SunNodes);

const ThermometerNodes = [
  ["path", { d: "M14 4v10.54a4 4 0 1 1-4 0V4a2 2 0 0 1 4 0Z", key: "17jzev" }],
] as const satisfies readonly IconNode[];
export const Thermometer = createIcon("thermometer", ThermometerNodes);

const TrendingUpNodes = [
  ["path", { d: "M16 7h6v6", key: "box55l" }],
  ["path", { d: "m22 7-8.5 8.5-5-5L2 17", key: "1t1m79" }],
] as const satisfies readonly IconNode[];
export const TrendingUp = createIcon("trending-up", TrendingUpNodes);

const UploadNodes = [
  ["path", { d: "M12 3v12", key: "1x0j5s" }],
  ["path", { d: "m17 8-5-5-5 5", key: "7q97r8" }],
  ["path", { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", key: "ih7n3h" }],
] as const satisfies readonly IconNode[];
export const Upload = createIcon("upload", UploadNodes);

const WifiNodes = [
  ["path", { d: "M12 20h.01", key: "zekei9" }],
  ["path", { d: "M2 8.82a15 15 0 0 1 20 0", key: "dnpr2z" }],
  ["path", { d: "M5 12.859a10 10 0 0 1 14 0", key: "1x1e6c" }],
  ["path", { d: "M8.5 16.429a5 5 0 0 1 7 0", key: "1bycff" }],
] as const satisfies readonly IconNode[];
export const Wifi = createIcon("wifi", WifiNodes);

const WifiOffNodes = [
  ["path", { d: "M12 20h.01", key: "zekei9" }],
  ["path", { d: "M8.5 16.429a5 5 0 0 1 7 0", key: "1bycff" }],
  ["path", { d: "M5 12.859a10 10 0 0 1 5.17-2.69", key: "1dl1wf" }],
  ["path", { d: "M19 12.859a10 10 0 0 0-2.007-1.523", key: "4k23kn" }],
  ["path", { d: "M2 8.82a15 15 0 0 1 4.177-2.643", key: "1grhjp" }],
  ["path", { d: "M22 8.82a15 15 0 0 0-11.288-3.764", key: "z3jwby" }],
  ["path", { d: "m2 2 20 20", key: "1ooewy" }],
] as const satisfies readonly IconNode[];
export const WifiOff = createIcon("wifi-off", WifiOffNodes);

const XNodes = [
  ["path", { d: "M18 6 6 18", key: "1bl5f8" }],
  ["path", { d: "m6 6 12 12", key: "d8bk6v" }],
] as const satisfies readonly IconNode[];
export const X = createIcon("x", XNodes);

const CircleXNodes = [
  ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
  ["path", { d: "m15 9-6 6", key: "1uzhvr" }],
  ["path", { d: "m9 9 6 6", key: "z0biqf" }],
] as const satisfies readonly IconNode[];
export const CircleX = createIcon("circle-x", CircleXNodes);

const ZapNodes = [
  [
    "path",
    {
      d: "M15.914 4a1.5 1.5 0 00-2.474-1.561l-9 9A1.5 1.5 0 005.5 14h4.002a.5.5 0 01.471.666L8.086 20a1.5 1.5 0 002.475 1.56l9-9A1.5 1.5 0 0018.5 10h-3.997a.5.5 0 01-.472-.667z",
      key: "1v7up4",
    },
  ],
] as const satisfies readonly IconNode[];
export const Zap = createIcon("zap", ZapNodes);

export const AlertCircle = CircleAlert;
export const AlertTriangle = TriangleAlert;
export const CheckCircle = CircleCheckBig;
export const CheckIcon = Check;
export const ChevronDownIcon = ChevronDown;
export const ChevronRightIcon = ChevronRight;
export const ChevronUpIcon = ChevronUp;
export const CircleIcon = Circle;
export const HelpCircle = CircleQuestionMark;
export const Home = House;
export const Loader2 = LoaderCircle;
export const XCircle = CircleX;
export const XIcon = X;
