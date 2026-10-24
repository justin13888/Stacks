// The notification channel form's model: a draft the form edits, checked
// against the config shapes the API declares (`NewChannelConfig`,
// `ChannelConfigPatch`), and turned into the request body that creates or
// changes a channel.

import type {
  Channel,
  ChannelConfigPatch,
  ChannelPatch,
  NewChannelBody,
  WebhookMethod,
} from "@/api/schemas";

export type ChannelType = "webhook" | "apprise";

export interface HeaderDraft {
  name: string;
  value: string;
}

export interface ChannelDraft {
  name: string;
  type: ChannelType;
  url: string;
  method: WebhookMethod;
  /** Only sent when `replaceHeaders` (always, for a new channel). */
  headers: HeaderDraft[];
  replaceHeaders: boolean;
  /** A new signing secret; blank keeps an existing one. */
  secret: string;
  /** Stop signing an existing channel's deliveries. */
  clearSecret: boolean;
  /** Apprise tags, comma-separated. */
  tags: string;
}

export type ChannelErrors = Partial<
  Record<"name" | "url" | "headers" | "secret", string>
>;

export function emptyDraft(): ChannelDraft {
  return {
    name: "",
    type: "webhook",
    url: "",
    method: "POST",
    headers: [],
    replaceHeaders: true,
    secret: "",
    clearSecret: false,
    tags: "",
  };
}

/** A draft for editing `channel`: its secrets and header values stay unread. */
export function draftOf(channel: Channel): ChannelDraft {
  const { config } = channel;
  return {
    ...emptyDraft(),
    name: channel.name,
    type: config.type,
    url: config.url,
    method: config.type === "webhook" ? config.method : "POST",
    headers: [],
    replaceHeaders: false,
    tags: config.type === "apprise" ? config.tags.join(", ") : "",
  };
}

/** A header field name (RFC 9110 `token`). */
const HEADER_NAME = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;

function isHttpUrl(text: string): boolean {
  try {
    const url = new URL(text);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

function tagList(text: string): string[] {
  return text
    .split(",")
    .map((tag) => tag.trim())
    .filter(Boolean);
}

function headerObject(headers: readonly HeaderDraft[]) {
  return Object.fromEntries(
    headers.map(({ name, value }) => [name.trim(), value]),
  );
}

/** What is wrong with `draft`, by field; empty when it can be sent. */
export function validateChannel(draft: ChannelDraft): ChannelErrors {
  const errors: ChannelErrors = {};
  const name = draft.name.trim();
  if (!name) errors.name = "Name the channel.";
  else if (name.length > 100) errors.name = "Keep the name to 100 characters.";
  if (!isHttpUrl(draft.url.trim())) {
    errors.url = "Enter an absolute http:// or https:// URL.";
  }
  if (draft.type === "webhook" && draft.replaceHeaders) {
    const names = draft.headers.map((h) => h.name.trim().toLowerCase());
    if (names.some((n) => !HEADER_NAME.test(n))) {
      errors.headers =
        "A header name can't be empty, or hold spaces or punctuation like : ( ) ,";
    } else if (new Set(names).size !== names.length) {
      errors.headers = "Each header can be set once.";
    }
  }
  return errors;
}

/** The body that creates a channel from a valid draft. */
export function toNewChannel(draft: ChannelDraft): NewChannelBody {
  const name = draft.name.trim();
  const url = draft.url.trim();
  if (draft.type === "apprise") {
    const tags = tagList(draft.tags);
    return {
      name,
      config: { type: "apprise", url, tags: tags.length ? tags : null },
    };
  }
  return {
    name,
    config: {
      type: "webhook",
      url,
      method: draft.method,
      headers: draft.headers.length ? headerObject(draft.headers) : null,
      secret: draft.secret ? draft.secret : null,
    },
  };
}

/**
 * The change from `channel` to `draft`, naming only what differs; `null`
 * when nothing does. A channel's type cannot change.
 */
export function toChannelPatch(
  channel: Channel,
  draft: ChannelDraft,
): ChannelPatch | null {
  const patch: ChannelPatch = {};
  const name = draft.name.trim();
  if (name !== channel.name) patch.name = name;
  const url = draft.url.trim();
  const { config } = channel;
  let configPatch: ChannelConfigPatch | null = null;
  if (config.type === "webhook") {
    const change: Extract<ChannelConfigPatch, { type: "webhook" }> = {
      type: "webhook",
    };
    if (url !== config.url) change.url = url;
    if (draft.method !== config.method) change.method = draft.method;
    if (draft.replaceHeaders) {
      change.headers = draft.headers.length
        ? headerObject(draft.headers)
        : null;
    }
    if (draft.secret) change.secret = draft.secret;
    else if (draft.clearSecret && config.has_secret) change.secret = null;
    if (Object.keys(change).length > 1) configPatch = change;
  } else {
    const change: Extract<ChannelConfigPatch, { type: "apprise" }> = {
      type: "apprise",
    };
    if (url !== config.url) change.url = url;
    const tags = tagList(draft.tags);
    if (tags.join(",") !== config.tags.join(",")) {
      change.tags = tags.length ? tags : null;
    }
    if (Object.keys(change).length > 1) configPatch = change;
  }
  if (configPatch) patch.config = configPatch;
  return Object.keys(patch).length ? patch : null;
}
