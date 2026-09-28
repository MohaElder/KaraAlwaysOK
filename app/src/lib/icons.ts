import type PlaylistIcon from "phosphor-svelte/lib/PlaylistIcon";
import YoutubeLogoIcon from "phosphor-svelte/lib/YoutubeLogoIcon";
import TelevisionSimpleIcon from "phosphor-svelte/lib/TelevisionSimpleIcon";
import type { WebSource } from "./api";

/** Any Phosphor icon component. */
export type Icon = typeof PlaylistIcon;

/** Each site's icon beside its search results. */
export const SITE_ICONS: Record<WebSource, Icon> = { youtube: YoutubeLogoIcon, bilibili: TelevisionSimpleIcon };
