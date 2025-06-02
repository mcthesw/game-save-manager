import { ElNotification } from 'element-plus'
import type { NotificationParams, NotificationHandle } from 'element-plus'
import { $t } from '../i18n'
import { ref } from 'vue'

/**
 * 通知类型，包括成功、警告、错误和信息
 */
type NotificationType = 'success' | 'warning' | 'error' | 'info'

/**
 * 通知选项接口，定义通知的基本属性
 */
interface NotificationOptions {
    /**
     * 可选的通知标题
     */
    title?: string
    /**
     * 通知显示时长
     */
    duration?: number
    /**
     * 通知消息内容
     */
    message: string
    /**
     * 是否持久显示通知
     */
    persistent?: boolean
}

/**
 * 队列中的通知接口，包含类型、选项和唯一标识符
 */
interface QueuedNotification {
    /**
     * 通知类型
     */
    type: NotificationType
    /**
     * 通知选项
     */
    options: NotificationOptions
    /**
     * 唯一标识符
     */
    id: string
}

/**
 * 活跃通知接口，记录当前显示的通知
 */
interface ActiveNotification {
    /**
     * 通知唯一标识符
     */
    id: string
    /**
     * 通知处理句柄
     */
    handle: NotificationHandle
}

/**
 * 消息队列，用于管理待显示的通知
 */
const messageQueue = ref<QueuedNotification[]>([])
/**
 * 当前活跃的通知列表
 */
const activeNotifications = ref<ActiveNotification[]>([])
/**
 * 处理状态标志
 */
let isProcessing = false

/**
 * 不同通知类型的默认显示时长
 */
const defaultDurations = {
    success: 3000,
    warning: 3000,
    error: 3000,
    info: 3000,
}

/**
 * 不同通知类型的默认标题（国际化）
 */
const defaultTitles = {
    success: $t('misc.success'),
    warning: $t('misc.warning'),
    error: $t('misc.error'),
    info: $t('misc.info'),
}

/**
 * 生成唯一标识符的函数
 */
const generateId = () => Math.random().toString(36).substring(2, 15)

/**
 * 显示通知的核心函数
 * @param type 通知类型
 * @param options 通知选项
 * @param id 通知唯一标识符
 * @returns 通知处理句柄
 */
const show = (type: NotificationType, options: NotificationOptions, id: string) => {
    const { message, title = defaultTitles[type], duration = options.persistent ? 0 : defaultDurations[type] } = options

    const handle = ElNotification({
        title,
        message,
        type,
        duration,
    } as NotificationParams)

    activeNotifications.value.push({ id, handle })
    return handle
}

/**
 * 处理通知队列的函数，确保通知按顺序显示
 */
const processQueue = async () => {
    if (isProcessing || messageQueue.value.length === 0) return

    isProcessing = true

    while (messageQueue.value.length > 0) {
        const notification = messageQueue.value.shift()
        if (notification) {
            show(notification.type, notification.options, notification.id)
            await new Promise(resolve => setTimeout(resolve, 100)) // 等待0.1秒
        }
    }

    isProcessing = false
}

/**
 * 将通知添加到队列的函数
 * @param type 通知类型
 * @param options 通知选项
 * @returns 通知的唯一标识符
 */
const addToQueue = (type: NotificationType, options: NotificationOptions): string => {
    const id = generateId()
    messageQueue.value.push({ type, options, id })
    processQueue()
    return id
}

/**
 * 关闭特定通知的函数
 * @param id 要关闭的通知的唯一标识符
 */
const closeNotification = (id: string) => {
    // 从队列中移除
    messageQueue.value = messageQueue.value.filter(item => item.id !== id)

    const index = activeNotifications.value.findIndex(n => n.id === id)
    if (index !== -1) {
        activeNotifications.value[index].handle.close()
        activeNotifications.value.splice(index, 1)
    }
}

/**
 * 提供通知相关的方法
 */
export function useNotification() {
    return {
        showSuccess: (options: NotificationOptions) => addToQueue('success', options),
        showWarning: (options: NotificationOptions) => addToQueue('warning', options),
        showError: (options: NotificationOptions) => addToQueue('error', options),
        showInfo: (options: NotificationOptions) => addToQueue('info', options),
        closeNotification,  // 导出关闭方法
    }
}