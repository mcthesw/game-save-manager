import { ElNotification } from "element-plus";
import { $t } from "../i18n";

function show_error(message: string, title = $t('misc.error'), duration = 3000) {
    return ElNotification({
        title,
        message,
        type: "error",
        duration: duration,
    });
}

function show_warning(message: string, title = $t('misc.warning'), duration = 3000) {
    return ElNotification({
        title,
        message,
        type: "warning",
        duration: duration,
    });
}

function show_success(message: string,title=$t('misc.success'),duration=1000) {
    return ElNotification({
        title,
        message,
        type: "success",
        duration: duration,
    });
}

function show_info(message: string,title=$t('misc.info'),duration=3000) {
    return ElNotification({
        title,
        message,
        type: "info",
        duration: duration,
    });
}

export {
    show_error,
    show_warning,
    show_success,
    show_info,
}