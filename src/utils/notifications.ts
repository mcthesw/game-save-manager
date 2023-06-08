import { ElNotification } from "element-plus";

function show_error(message: string) {
    return ElNotification({
        title: "错误",
        message,
        type: "error",
        duration: 3000,
    });
}

function show_warning(message: string) {
    return ElNotification({
        title: "警告",
        message,
        type: "warning",
        duration: 3000,
    });
}

function show_success(message: string) {
    return ElNotification({
        title: "成功",
        message,
        type: "success",
        duration: 1000,
    });
}

function show_info(message: string) {
    return ElNotification({
        title: "信息提示",
        message,
        type: "info",
        duration: 3000,
    });
}

export {
    show_error,
    show_warning,
    show_success,
    show_info,
}