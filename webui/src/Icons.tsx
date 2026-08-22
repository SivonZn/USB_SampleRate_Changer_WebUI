import type { JSX } from "solid-js";

type IconProps = { class?: string };

function Icon(props: IconProps & { children: JSX.Element }) {
  return (
    <svg
      class={props.class}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {props.children}
    </svg>
  );
}

export function BrandIcon(props: IconProps) {
  return <Icon {...props}><path d="M8 3v7m8-7v7M6 10h12v3a6 6 0 0 1-12 0v-3Z"/><path d="M12 19v2"/></Icon>;
}

export function PolicyIcon(props: IconProps) {
  return <Icon {...props}><path d="M4 7h10M18 7h2M4 17h2m4 0h10M14 4v6M6 14v6"/></Icon>;
}

export function ToolsIcon(props: IconProps) {
  return <Icon {...props}><path d="m14.7 6.3 3-3a4 4 0 0 1-5 5l-7.4 7.4a2.1 2.1 0 1 0 3 3l7.4-7.4a4 4 0 0 1 5-5l-3 3"/></Icon>;
}

export function TuneIcon(props: IconProps) {
  return <Icon {...props}><path d="M12 2.5c.6 4.9 4.6 8.9 9.5 9.5-4.9.6-8.9 4.6-9.5 9.5-.6-4.9-4.6-8.9-9.5-9.5 4.9-.6 8.9-4.6 9.5-9.5Z"/></Icon>;
}

export function SettingsIcon(props: IconProps) {
  return <Icon {...props}><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.09a2 2 0 0 1 1 1.74v.5a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.38a2 2 0 0 0-.73-2.73l-.15-.09a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2Z"/><circle cx="12" cy="12" r="3"/></Icon>;
}

export function RefreshIcon(props: IconProps) {
  return <Icon {...props}><path d="M20 6v5h-5M4 18v-5h5"/><path d="M18.5 9A7 7 0 0 0 6 6.5L4 9m16 6-2 2.5A7 7 0 0 1 5.5 15"/></Icon>;
}

export function LogIcon(props: IconProps) {
  return <Icon {...props}><path d="M6 3h9l3 3v15H6z"/><path d="M14 3v4h4M9 12h6m-6 4h6"/></Icon>;
}

export function CheckIcon(props: IconProps) {
  return <Icon {...props}><path d="m5 12 4 4L19 6"/></Icon>;
}

export function WarningIcon(props: IconProps) {
  return <Icon {...props}><path d="M12 3 2.7 20h18.6L12 3Z"/><path d="M12 9v4m0 3h.01"/></Icon>;
}
