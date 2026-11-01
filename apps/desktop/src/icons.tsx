import type { SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

function base({ size = 16, ...props }: IconProps, children: React.ReactNode) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {children}
    </svg>
  );
}

export const GridIcon = (p: IconProps) =>
  base(p, <><rect x="4" y="4" width="6" height="6" rx="1.2" /><rect x="14" y="4" width="6" height="6" rx="1.2" /><rect x="4" y="14" width="6" height="6" rx="1.2" /><rect x="14" y="14" width="6" height="6" rx="1.2" /></>);
export const SearchIcon = (p: IconProps) => base(p, <><circle cx="11" cy="11" r="6.5" /><path d="m16 16 4 4" /></>);
export const PlusIcon = (p: IconProps) => base(p, <path d="M12 5v14M5 12h14" />);
export const FileIcon = (p: IconProps) =>
  base(p, <><path d="M7 3.5h7l4 4V20a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1Z" /><path d="M14 3.5V8h4" /></>);
export const LinkIcon = (p: IconProps) =>
  base(p, <><path d="M10 13a5 5 0 0 0 7.1 0l1.4-1.4a5 5 0 0 0-7.1-7.1L10 5.9" /><path d="M14 11a5 5 0 0 0-7.1 0l-1.4 1.4a5 5 0 0 0 7.1 7.1l1.4-1.4" /></>);
export const SettingsIcon = (p: IconProps) =>
  base(p, <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1A1.7 1.7 0 0 0 9 4.6a1.7 1.7 0 0 0 1-1.6v-.2h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z" /></>);
export const UserIcon = (p: IconProps) => base(p, <><circle cx="12" cy="8" r="3.5" /><path d="M5 20c0-3.4 3.1-5.5 7-5.5s7 2.1 7 5.5" /></>);
export const FunnelIcon = (p: IconProps) => base(p, <path d="M4 5h16l-6.2 7.2V20l-3.6-2v-5.8L4 5Z" />);
export const SparkleIcon = (p: IconProps) => base(p, <path d="M12 3.5 13.7 9l5.5 1.7-5.5 1.7L12 18l-1.7-5.6L4.8 10.7 10.3 9 12 3.5Z" />);
export const PencilIcon = (p: IconProps) => base(p, <><path d="m4 20 4.2-1 10.9-10.9a2 2 0 0 0 0-2.8l-.4-.4a2 2 0 0 0-2.8 0L5 15.8 4 20Z" /><path d="m13.8 6.2 4 4" /></>);
export const ChevronIcon = (p: IconProps) => base(p, <path d="m9 6 6 6-6 6" />);
export const CheckIcon = (p: IconProps) => base(p, <path d="m5 12.5 4.2 4.2L19 7" />);
export const EllipsisIcon = (p: IconProps) => base(p, <><circle cx="5" cy="12" r="1" fill="currentColor" /><circle cx="12" cy="12" r="1" fill="currentColor" /><circle cx="19" cy="12" r="1" fill="currentColor" /></>);
export const PanelIcon = (p: IconProps) => base(p, <><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M15 4v16" /></>);
export const MicIcon = (p: IconProps) => base(p, <><rect x="9" y="3" width="6" height="11" rx="3" /><path d="M5 11a7 7 0 0 0 14 0M12 18v3" /></>);
export const SendIcon = (p: IconProps) => base(p, <><path d="m4 12 16-8-5.5 16-3-6.5L4 12Z" /><path d="M11.5 13.5 20 4" /></>);
export const HistoryIcon = (p: IconProps) => base(p, <><path d="M4 12a8 8 0 1 0 2.3-5.7" /><path d="M4 4v4h4" /><path d="M12 8v4l2.5 1.5" /></>);
export const AlertIcon = (p: IconProps) => base(p, <><path d="M12 4 3.5 19h17L12 4Z" /><path d="M12 9v4" /><circle cx="12" cy="16.5" r=".7" fill="currentColor" /></>);
export const TargetIcon = (p: IconProps) => base(p, <><circle cx="12" cy="12" r="7.5" /><circle cx="12" cy="12" r="3" /></>);
export const CircleIcon = (p: IconProps) => base(p, <circle cx="12" cy="12" r="7.5" />);
export const NoteIcon = (p: IconProps) => base(p, <><path d="M6 4h9l3 3v13H6V4Z" /><path d="M8.5 9h7M8.5 13h7M8.5 17h4" /></>);
export const BranchIcon = (p: IconProps) => base(p, <><circle cx="6" cy="5" r="2" /><circle cx="18" cy="19" r="2" /><path d="M6 7v4a4 4 0 0 0 4 4h4a4 4 0 0 1 4 4" /></>);
export const SunIcon = (p: IconProps) =>
  base(p, <><circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" /></>);
export const MoonIcon = (p: IconProps) => base(p, <path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5Z" />);
export const LanguagesIcon = (p: IconProps) =>
  base(p, <><path d="M4 5h9M8.5 3v2M6 9c1.5 2.5 3.5 4 6 4.5M12 5c-.5 4-2.5 7.5-6 10" /><path d="m12.5 20 3.5-8 3.5 8M13.8 17.2h4.4" /></>);
export const WindowIcon = (p: IconProps) => base(p, <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M3 9h18M8 5V3M16 5V3" /></>);
export const LockIcon = (p: IconProps) =>
  base(p, <><rect x="5" y="10" width="14" height="10" rx="2" /><path d="M8 10V7a4 4 0 0 1 8 0v3M12 14v2" /></>);
export const CloudIcon = (p: IconProps) => base(p, <path d="M7 18a4 4 0 0 1-.7-7.9A5.5 5.5 0 0 1 17 8.5 3.5 3.5 0 0 1 18.5 18H7Z" />);
export const PlayIcon = (p: IconProps) => base(p, <path d="M8 5.5v13l10-6.5-10-6.5Z" />);
export const GlobeIcon = (p: IconProps) =>
  base(p, <><circle cx="12" cy="12" r="8.5" /><path d="M3.5 12h17M12 3.5c2.5 2.3 3.6 5.2 3.6 8.5s-1.1 6.2-3.6 8.5c-2.5-2.3-3.6-5.2-3.6-8.5S9.5 5.8 12 3.5Z" /></>);
export const FlaskIcon = (p: IconProps) =>
  base(p, <><path d="M9 3h6M10 3v6l-5 9a2 2 0 0 0 1.8 3h10.4a2 2 0 0 0 1.8-3l-5-9V3" /><path d="M7.5 14h9" /></>);
export const LayersIcon = (p: IconProps) =>
  base(p, <><path d="m12 3 9 5-9 5-9-5 9-5Z" /><path d="m3 13 9 5 9-5M3 17l9 5 9-5" /></>);
export const BoxIcon = (p: IconProps) =>
  base(p, <><path d="m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Z" /><path d="m4 7.5 8 4.5 8-4.5M12 12v9" /></>);
export const ArrowRightIcon = (p: IconProps) => base(p, <path d="M4 12h16M14 6l6 6-6 6" />);
export const CloseIcon = (p: IconProps) => base(p, <path d="m6 6 12 12M18 6 6 18" />);
export const ShieldIcon = (p: IconProps) =>
  base(p, <path d="M12 3 5 6v5c0 4.5 3 7.8 7 10 4-2.2 7-5.5 7-10V6l-7-3Z" />);
