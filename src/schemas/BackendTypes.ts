/**
 * 所有可能的后端类型
 */
export type Backend = Disabled | WebDAV;

export type Disabled = { type: "Disabled", };

export type WebDAV = {
    type: "WebDAV",
    endpoint: string,
    username: string,
    password: string,
}