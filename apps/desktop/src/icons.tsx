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
