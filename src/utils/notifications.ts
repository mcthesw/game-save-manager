import { ElNotification } from "element-plus";
import { $t } from "../i18n";

function show_error(message: string,title=$t('misc.error')) {
    return ElNotification({
        title,
        message,
        type: "error",
        duration: 3000,
    });
}

function show_warning(message: string,title=$t('misc.warning')) {
    return ElNotification({
        title,
        message,
        type: "warning",
        duration: 3000,
    });
}

function show_success(message: string,title=$t('misc.success')) {
    return ElNotification({
        title,
        message,
        type: "success",
        duration: 1000,
    });
}

function show_info(message: string,title=$t('misc.info')) {
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