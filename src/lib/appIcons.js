import normal from "../assets/icons/normal.png";
import blue from "../assets/icons/blue.png";
import red from "../assets/icons/red.png";
import orange from "../assets/icons/orange.png";
import yellow from "../assets/icons/yellow.png";
import crt from "../assets/icons/crt.png";
import frutigerAero from "../assets/icons/frutiger-aero.png";
import sunshineRainbows from "../assets/icons/sunshine-rainbows.png";

// Pre-made variants only. Ids must match the list in src-tauri/src/commands/app_icon.rs.
export const APP_ICONS = [
  { id: "normal", url: normal },
  { id: "blue", url: blue },
  { id: "red", url: red },
  { id: "orange", url: orange },
  { id: "yellow", url: yellow },
  { id: "crt", url: crt },
  { id: "frutiger-aero", url: frutigerAero },
  { id: "sunshine-rainbows", url: sunshineRainbows },
];

export const DEFAULT_APP_ICON = "normal";

export function appIconUrl(id) {
  return (APP_ICONS.find((icon) => icon.id === id) ?? APP_ICONS[0]).url;
}
