/**
 * 所有可能的后端类型
 */
export type Backend = Disabled | WebDAV | S3;

export const backends = ["Disabled", "WebDAV", "S3"] // 可用的后端类型

export type Disabled = { type: "Disabled", };

export type WebDAV = {
    type: "WebDAV",
    endpoint: string,
    username: string,
    password: string,
}

export type S3 = {
    type: "S3",
    endpoint: string,
    bucket: string,
    region: string,
    access_key_id: string,
    secret_access_key: string,
}