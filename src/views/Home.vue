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
                    <el-button type="primary" @click="go_add_game()">{{ $t('home.go_add_game') }}</el-button>
                </template>
            </el-result>
            <el-result :title="$t('home.go_save_manage')">
                <template #icon>
                    <Files />
                </template>
                <template #extra>
                    <el-button type="primary" @click="go_game_manage()">{{ $t('home.hint') }}</el-button>
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
function go_game_manage() {
    show_info($t('home.go_game_manage_hint'))
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
</style>