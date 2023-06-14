export interface EventWrapper<T> {
    payload:T
}
export type NotificationLevel = "info" | "warning" | "error"
export interface IpcNotification {
    level: NotificationLevel,
    title: string,
    msg: string,
}