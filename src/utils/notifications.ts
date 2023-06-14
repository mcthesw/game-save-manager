import { ElNotification } from "element-plus";

function show_error(message: string,title="错误") {
    return ElNotification({
        title,
        message,
        type: "error",
        duration: 3000,
    });
}

function show_warning(message: string,title="警告") {
    return ElNotification({
        title,
        message,
        type: "warning",
        duration: 3000,
    });
}

function show_success(message: string,title="成功") {
    return ElNotification({
        title,
        message,
        type: "success",
        duration: 1000,
    });
}

function show_info(message: string,title="信息提示") {
    return ElNotification({
        title,
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