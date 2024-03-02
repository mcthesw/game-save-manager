<script setup lang="ts">
// 同步使用方法：配置好后，首先覆盖性上传，然后打开随时同步
// 之后每次启动该软件，如果在其他机子做过修改，应当手动从云端下载，用新的数据覆盖本地
// 如果没有，则不需要任何操作，之后更新了自动同步功能就可以启动时自动下载，避免手动操作

import { ref } from "vue";
import { useConfig } from "../stores/ConfigFile";
import { invoke } from "@tauri-apps/api/tauri";
import { show_error, show_info, show_success } from "../utils/notifications";
import { CloudSettings } from "../schemas/saveTypes";
import { $t } from "../i18n";
import { ElButton, ElCard, ElContainer, ElForm, ElFormItem, ElInput, ElInputNumber, ElLink, ElMessageBox, ElOption, ElSelect, ElSwitch } from "element-plus";
import { Ref } from "vue";
import type { Backend, WebDAV } from "../schemas/BackendTypes";

const config = useConfig() // 配置文件
const backends = ["Disabled", "WebDAV",] // 可用的后端类型
const cloud_settings: Ref<CloudSettings> = ref(config.settings.cloud_settings) // 云同步配置

const webdav_settings: Ref<WebDAV> = ref({
  type: "WebDAV",
  endpoint: "",
  username: "",
  password: ""
} as WebDAV)
// 从配置中加载云同步配置，这个重复步骤是必要的，因为我们无法确定用户的当前配置
if (cloud_settings.value.backend.type === "WebDAV") {
  webdav_settings.value = cloud_settings.value.backend as WebDAV
}

/**
 * 测试同步后端是否可用
 */
function check() {
  show_info($t("sync_settings.start_test"))
  switch (cloud_settings.value.backend.type) {
    case "Disabled":
      show_error($t("sync_settings.test_failed"))
      break
    case "WebDAV":
      if (webdav_settings.value.endpoint.endsWith("/")) {
        // 去掉末尾的斜杠，防止出现重复的斜杠
        webdav_settings.value.endpoint = webdav_settings.value.endpoint.slice(0, -1)
      }
      invoke("check_cloud_backend", { backend: webdav_settings.value }).then((res) => {
        show_success($t("sync_settings.test_success"))
      }).catch((err) => {
        show_error($t("sync_settings.test_failed"))
        console.error("WebDAV test error:", err)
      })
      break
    default:
      show_error($t("sync_settings.unknown_backend"))
  }
}

function save() {
  switch (cloud_settings.value.backend.type) {
    case "Disabled":
      cloud_settings.value.backend = { type: "Disabled" } as Backend
      break
    case "WebDAV":
      cloud_settings.value.backend = webdav_settings.value
      if (cloud_settings.value.backend.endpoint.endsWith("/")) {
        // 去掉末尾的斜杠，防止出现重复的斜杠
        cloud_settings.value.backend.endpoint = cloud_settings.value.backend.endpoint.slice(0, -1)
      }
      break
    default:
      show_error($t("sync_settings.unknown_backend"))
      return
  }
  // 应用暂存的云同步配置
  config.settings.cloud_settings = cloud_settings.value
  submit_settings()
}

function load_config() {
  config.refresh()
  // 重新加载临时配置
  cloud_settings.value = config.settings.cloud_settings
}
/**
 * 提交配置，不应独立调用，需使用save函数调用，否则临时配置不会覆盖到配置中
 */
function submit_settings() {
  invoke("set_config", { config: config.$state }).then((x) => {
    show_success($t("sync_settings.submit_success"));
    load_config()
  }).catch(
    (e) => {
      console.log(e)
      show_error($t("error.set_config_failed"))
    }
  )
}
function abort_change() {
  show_success($t("sync_settings.reset_success"));
  load_config();
}

function upload_all() {
  ElMessageBox.prompt(
    $t("sync_settings.confirm_upload_all"),
    $t('home.hint'),
    {
      confirmButtonText: $t('sync_settings.confirm'),
      cancelButtonText: $t('sync_settings.cancel'),
      inputPattern: /yes/,
      inputErrorMessage: $t('sync_settings.invalid_input_error'),
    }
  ).then(() => {
    invoke("cloud_upload_all", { backend: webdav_settings.value }).then((res) => {
      show_success($t("sync_settings.upload_success"))
    }).catch((err) => {
      show_error($t("sync_settings.upload_failed"))
      console.error("Upload error:", err)
    })
  }).catch((e) => {
    show_info($t("sync_settings.canceled"))
  })
}

function download_all() {
  ElMessageBox.prompt(
    $t("sync_settings.confirm_download_all"),
    $t('home.hint'),
    {
      confirmButtonText: $t('sync_settings.confirm'),
      cancelButtonText: $t('sync_settings.cancel'),
      inputPattern: /yes/,
      inputErrorMessage: $t('sync_settings.invalid_input_error'),
    }
  ).then(() => {
    invoke("cloud_download_all", { backend: webdav_settings.value }).then((res) => {
      show_success($t("sync_settings.download_success"))
    }).catch((err) => {
      show_error($t("sync_settings.download_failed"))
      console.error("Download error:", err)
    })
  }).catch((e) => {
    show_info($t("sync_settings.canceled"))
  })
}

function open_manual() {
  invoke("open_url", { url: "https://help.sworld.club/docs/extras/cloud" }).catch(
    (e) => {
      console.log(e)
      show_error($t("error.open_url_failed"))
    }
  )
}
</script>

<template>
  <ElContainer direction="vertical">
    <ElCard>
      <h1>{{ $t("sync_settings.title") }}</h1>
      <p class="bold">
        {{ $t("sync_settings.warning") }}
        <ElLink @click="open_manual">{{ $t("sync_settings.manual_link") }}</ElLink>
      </p>
      <p class="hint">{{ $t("sync_settings.notification") }}</p>
      <ElForm label-position="left" :label-width="120">
        <ElFormItem :label="$t('sync_settings.always_sync')">
          <ElSwitch v-model="cloud_settings.always_sync" />
          <span class="hint">{{ $t("sync_settings.always_sync_hint") }}</span>
        </ElFormItem>
        <ElFormItem :label="$t('sync_settings.auto_sync_interval')">
          <ElInputNumber :disabled="true" :value-on-clear="0" :step="1" :step-strictly="true"
            :min="0" />
          <span class="hint">{{ $t('sync_settings.interval_hint') }}</span>
        </ElFormItem>
        <ElFormItem :label="$t('sync_settings.cloud_root')">
          <ElInput v-model="cloud_settings.root_path" />
          <span class="hint">{{ $t('sync_settings.cloud_root_hint') }}</span>
        </ElFormItem>
        <ElFormItem :label="$t('sync_settings.backend')">
          <ElSelect :placeholder="$t('sync_settings.backend')" v-model="cloud_settings.backend.type">
            <ElOption v-for="backend in backends" :key="backend" :label="backend" :value="backend" />
          </ElSelect>
          <span class="hint">{{ $t('sync_settings.backend_hint') }}</span>
        </ElFormItem>
        <!-- WebDAV start -->
        <template v-if="cloud_settings.backend.type === 'WebDAV'">
          <ElFormItem :label="$t('sync_settings.webdav.endpoint')">
            <ElInput v-model="webdav_settings.endpoint" />
          </ElFormItem>
          <ElFormItem :label="$t('sync_settings.webdav.username')">
            <ElInput v-model="webdav_settings.username" />
          </ElFormItem>
          <ElFormItem :label="$t('sync_settings.webdav.password')">
            <ElInput type="password" v-model="webdav_settings.password" />
          </ElFormItem>
        </template>
        <!-- WebDAV end -->
        <ElFormItem>
          <ElButton @click="save">{{ $t("sync_settings.save_button") }}</ElButton>
          <ElButton @click="abort_change">{{ $t("sync_settings.abort_button") }}</ElButton>
          <ElButton @click="check">{{ $t("sync_settings.test_button") }}</ElButton>
        </ElFormItem>
        <ElFormItem>
          <ElButton type="danger" @click="upload_all">覆盖性上传</ElButton>
          <ElButton type="danger" @click="download_all">覆盖性下载</ElButton>
        </ElFormItem>
      </ElForm>
    </ElCard>
  </ElContainer>
</template>

<style scoped>
.hint {
  margin-left: 10px;
  color: #808080;
}

.bold {
  margin-left: 10px;
  font-weight: bold;
  color: #000000;
}

.el-input {
  width: 300px;
}

.el-switch {
  margin-right: 20px;
}

label {
  margin-right: 20px;
}

.el-row {
  margin-top: 20px;
}
</style>
