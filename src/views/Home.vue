<template>
    <el-container class="home-container" direction="vertical">
        <h2>{{ $t('home.hello_world') }}</h2>
        <div class="describe-container">
            <div class="describe">
                <h3>{{ $t('home.simple') }}</h3>
                <p>{{ $t('home.simple_explained') }}</p>
            </div>
            <div class="describe">
                <h3>{{ $t('home.safe') }}</h3>
                <p>
                    {{ $t('home.safe_explained') }} </p>
            </div>
            <div class="describe">
                <h3>{{ $t('home.free') }}</h3>
                <p>
                    {{ $t('home.free_explained') }} </p>
            </div>
        </div>
        <el-container class="new" direction="horizontal">
            <el-result :title="$t('home.import_game')">
                <template #icon>
                    <Edit />
                </template>
                <template #extra>
                    <el-button type="primary" @click="go_add_game()">{{ $t('home.jump_to_page') }}</el-button>
                </template>
            </el-result>
            <el-result :title="$t('home.change_locale')">
                <template #icon>
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 512">
                        <path
                            d="M152.1 236.2c-3.5-12.1-7.8-33.2-7.8-33.2h-.5s-4.3 21.1-7.8 33.2l-11.1 37.5H163zM616 96H336v320h280c13.3 0 24-10.7 24-24V120c0-13.3-10.7-24-24-24zm-24 120c0 6.6-5.4 12-12 12h-11.4c-6.9 23.6-21.7 47.4-42.7 69.9 8.4 6.4 17.1 12.5 26.1 18 5.5 3.4 7.3 10.5 4.1 16.2l-7.9 13.9c-3.4 5.9-10.9 7.8-16.7 4.3-12.6-7.8-24.5-16.1-35.4-24.9-10.9 8.7-22.7 17.1-35.4 24.9-5.8 3.5-13.3 1.6-16.7-4.3l-7.9-13.9c-3.2-5.6-1.4-12.8 4.2-16.2 9.3-5.7 18-11.7 26.1-18-7.9-8.4-14.9-17-21-25.7-4-5.7-2.2-13.6 3.7-17.1l6.5-3.9 7.3-4.3c5.4-3.2 12.4-1.7 16 3.4 5 7 10.8 14 17.4 20.9 13.5-14.2 23.8-28.9 30-43.2H412c-6.6 0-12-5.4-12-12v-16c0-6.6 5.4-12 12-12h64v-16c0-6.6 5.4-12 12-12h16c6.6 0 12 5.4 12 12v16h64c6.6 0 12 5.4 12 12zM0 120v272c0 13.3 10.7 24 24 24h280V96H24c-13.3 0-24 10.7-24 24zm58.9 216.1L116.4 167c1.7-4.9 6.2-8.1 11.4-8.1h32.5c5.1 0 9.7 3.3 11.4 8.1l57.5 169.1c2.6 7.8-3.1 15.9-11.4 15.9h-22.9a12 12 0 0 1 -11.5-8.6l-9.4-31.9h-60.2l-9.1 31.8c-1.5 5.1-6.2 8.7-11.5 8.7H70.3c-8.2 0-14-8.1-11.4-15.9z" />
                    </svg>
                </template>
                <template #extra>
                    <el-button type="primary" @click="go_settings()">{{ $t('home.jump_to_page') }}</el-button>
                </template>
            </el-result>
            <el-result :title="$t('home.start_backup')">
                <template #icon>
                    <UploadFilled />
                </template>
                <template #extra>
                    <el-button type="primary" @click="go_backup()">{{ $t('home.hint') }}</el-button>
                </template>
            </el-result>
        </el-container>
    </el-container>
</template>

<script lang="ts" setup>
import { Edit, UploadFilled, Files } from "@element-plus/icons-vue";
import { useRouter } from "vue-router";
import { show_info } from "../utils/notifications";
import { $t } from "../i18n";
const router = useRouter();

function go_add_game() {
    router.push("/add-game");
}
function go_settings() {
    router.push("/settings")
}
function go_backup() {
    show_info($t('home.go_backup_hint'));
}
</script>

<style>
.home-container {
    height: 95%;
}

.home-container>h2 {
    margin-right: auto;
    margin-left: auto;
    margin-bottom: 2em;
    display: block;
    font-size: 2em;
}

.describe-container {
    display: flex;
    justify-content: space-around;
}

.describe {
    width: 25%;
}

.new {
    margin-top: 100px;
    justify-content: space-around;
}

svg {
    fill: var(--el-text-color-primary);
}
</style>