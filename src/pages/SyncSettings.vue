<script setup lang="ts">
// 同步使用方法：配置好后，首先覆盖性上传，然后打开随时同步
// 之后每次启动该软件，如果在其他机子做过修改，应当手动从云端下载，用新的数据覆盖本地
// 如果没有，则不需要任何操作，之后更新了自动同步功能就可以启动时自动下载，避免手动操作

import { ref } from "vue";
import { $t } from "../i18n";
import { commands, type Backend } from "../bindings";
import { error } from "@tauri-apps/plugin-log";

interface WebDAV {
  type: "WebDAV";
  endpoint: string;
  username: string;
  password: string;
}
interface S3 {
  type: "S3";
  endpoint: string;
  bucket: string;
  region: string;
  access_key_id: string;
  secret_access_key: string;
}
const backends = ["WebDAV", "S3", "Disabled"]

const { config, refreshConfig, saveConfig } = useConfig() // 配置文件
const cloud_settings = ref(config.value!.settings.cloud_settings) // 云同步配置
const { showInfo, showError, showSuccess } = useNotification()
const { withLoading } = useGlobalLoading()

const webdav_settings: Ref<WebDAV> = ref({
  type: "WebDAV",
  endpoint: "",
  username: "",
  password: ""
} as WebDAV)
const s3_settings: Ref<S3> = ref({
  type: "S3",
  endpoint: "",
  bucket: "",
  region: "",
  access_key_id: "",
  secret_access_key: "",
} as S3)
// 从配置中加载云同步配置，这个重复步骤是必要的，因为我们无法确定用户的当前配置
switch (cloud_settings.value!.backend!.type) {
  case "WebDAV":
    webdav_settings.value = cloud_settings.value!.backend as WebDAV;
    break;
  case "S3":
    s3_settings.value = cloud_settings.value!.backend as S3;
    break;
  default:
    showError({ message: $t("sync_settings.unknown_backend") }) // TODO:更换成更合适的提醒
    break;
}

/**
 * 测试同步后端是否可用
 */
async function check() {
  showInfo({ message: $t("sync_settings.start_test") })
  await withLoading(async () => {
    switch (cloud_settings.value?.backend!.type) {
      case "Disabled":
        showError({ message: $t("sync_settings.test_failed") })
        break
      case "WebDAV":
        if (webdav_settings.value.endpoint.endsWith("/")) {
          // 去掉末尾的斜杠，防止出现重复的斜杠
          webdav_settings.value.endpoint = webdav_settings.value.endpoint.slice(0, -1)
        }
        const webdavResult = await commands.checkCloudBackend(webdav_settings.value)
        if (webdavResult.status === "error") {
          showError({ message: $t("sync_settings.test_failed") })
          error(`WebDAV test error: ${webdavResult.error}`)
        } else {
          showSuccess({ message: $t("sync_settings.test_success") })
        }
        break
      case "S3":
        if (s3_settings.value.endpoint.endsWith("/")) {
          // 去掉末尾的斜杠，防止出现重复的斜杠
          s3_settings.value.endpoint = s3_settings.value.endpoint.slice(0, -1)
        }
        const s3Result = await commands.checkCloudBackend(s3_settings.value)
        if (s3Result.status === "error") {
          showError({ message: $t("sync_settings.test_failed") })
          error(`S3 test error: ${s3Result.error}`)
        } else {
          showSuccess({ message: $t("sync_settings.test_success") })
        }
        break;
      default:
        showError({ message: $t("sync_settings.unknown_backend") })
    }
  }, $t('sync_settings.checking_backend'))
}

function save() {
  switch (cloud_settings.value?.backend!.type) {
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
    case "S3":
      cloud_settings.value.backend = s3_settings.value
      if (cloud_settings.value.backend.endpoint.endsWith("/")) {
        cloud_settings.value.backend.endpoint = cloud_settings.value.backend.endpoint.slice(0, -1)
      }
      break
    default:
      showError({ message: $t("sync_settings.unknown_backend") })
      return
  }
  // 应用暂存的云同步配置
  config.value!.settings.cloud_settings = cloud_settings.value
  submit_settings()
}

async function load_config() {
  await refreshConfig()
  // 重新加载临时配置
  cloud_settings.value = config.value!.settings.cloud_settings
}
/**
 * 提交配置，不应独立调用，需使用save函数调用，否则临时配置不会覆盖到配置中
 */
async function submit_settings() {
  try {
    await saveConfig();
    showSuccess({ message: $t("sync_settings.submit_success") });
    await load_config();
  } catch (e) {
    error(`Failed to set config: ${e}`);
    showError({ message: $t("error.set_config_failed") });
  }
}
function abort_change() {
  showSuccess({ message: $t("sync_settings.reset_success") });
  load_config();
}

async function upload_all() {
  try {
    await ElMessageBox.prompt(
      $t("sync_settings.confirm_upload_all"),
      $t('home.hint'),
      {
        confirmButtonText: $t('sync_settings.confirm'),
        cancelButtonText: $t('sync_settings.cancel'),
        inputPattern: /yes/,
        inputErrorMessage: $t('sync_settings.invalid_input_error'),
      }
    );

    const result = await withLoading(async () => {
      return await commands.cloudUploadAll(config.value!.settings.cloud_settings!.backend!);
    }, $t('sync_settings.uploading_all'));
    if (result.status === "error") {
      showError({ message: $t("sync_settings.upload_failed") });
      error(`Upload error: ${result.error}`);
    } else {
      showSuccess({ message: $t("sync_settings.upload_success") });
    }
  } catch (err) {
    // 这里处理的是 ElMessageBox.prompt 的取消操作
    showInfo({ message: $t("sync_settings.canceled") });
  }
}

async function download_all() {
  try {
    await ElMessageBox.prompt(
      $t("sync_settings.confirm_download_all"),
      $t('home.hint'),
      {
        confirmButtonText: $t('sync_settings.confirm'),
        cancelButtonText: $t('sync_settings.cancel'),
        inputPattern: /yes/,
        inputErrorMessage: $t('sync_settings.invalid_input_error'),
      }
    );

    const result = await withLoading(async () => {
      return await commands.cloudDownloadAll(config.value!.settings.cloud_settings!.backend!);
    }, $t('sync_settings.downloading_all'));
    if (result.status === "error") {
      showError({ message: $t("sync_settings.download_failed") });
      error(`Download error: ${result.error}`);
    } else {
      showSuccess({ message: $t("sync_settings.download_success") });
    }
  } catch (e) {
    // 这里处理的是 ElMessageBox.prompt 的取消操作
    showInfo({ message: $t("sync_settings.canceled") });
  }
}

async function open_manual() {
  const result = await commands.openUrl("https://help.sworld.club/docs/extras/cloud")
  if (result.status === "error") {
    error(`open manual error: ${result.error}`)
    showError({ message: $t("error.open_url_failed") })
  }
}
</script>

<template>
  <div>
    <ElCard>
      <h1>{{ $t("sync_settings.title") }}</h1>
      <p class="bold">
        {{ $t("sync_settings.warning") }}
        <ElLink @click="open_manual">{{ $t("sync_settings.manual_link") }}</ElLink>
      </p>
      <p class="hint">{{ $t("sync_settings.notification") }}</p>
      <ElForm label-position="left" :label-width="120">
        <ElFormItem :label="$t('sync_settings.always_sync')">
          <ElSwitch v-model="cloud_settings!.always_sync" />
          <span class="hint">{{ $t("sync_settings.always_sync_hint") }}</span>
        </ElFormItem>
        <ElFormItem :label="$t('sync_settings.auto_sync_interval')">
          <ElInputNumber :disabled="true" :value-on-clear="0" :step="1" :step-strictly="true" :min="0" />
          <span class="hint">{{ $t('sync_settings.interval_hint') }}</span>
        </ElFormItem>
        <ElFormItem :label="$t('sync_settings.cloud_root')">
          <ElInput v-model="cloud_settings!.root_path" />
          <span class="hint">{{ $t('sync_settings.cloud_root_hint') }}</span>
        </ElFormItem>
        <ElFormItem :label="$t('sync_settings.backend')">
          <ElSelect :placeholder="$t('sync_settings.backend')" v-model="cloud_settings!.backend!.type">
            <ElOption v-for="backend in backends" :key="backend" :label="backend" :value="backend" />
          </ElSelect>
          <span class="hint">{{ $t('sync_settings.backend_hint') }}</span>
        </ElFormItem>
        <!-- WebDAV start -->
        <template v-if="cloud_settings!.backend!.type === 'WebDAV'">
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
        <!-- S3 start -->
        <template v-if="cloud_settings!.backend!.type === 'S3'">
          <ElFormItem :label="$t('sync_settings.s3.endpoint')">
            <ElInput v-model="s3_settings.endpoint" />
          </ElFormItem>
          <ElFormItem :label="$t('sync_settings.s3.bucket')">
            <ElInput v-model="s3_settings.bucket" />
          </ElFormItem>
          <ElFormItem :label="$t('sync_settings.s3.region')">
            <ElInput v-model="s3_settings.region" />
            <span class="hint">{{ $t('sync_settings.s3.region_hint') }}</span>
          </ElFormItem>
          <ElFormItem :label="$t('sync_settings.s3.access_key_id')">
            <ElInput v-model="s3_settings.access_key_id" />
          </ElFormItem>
          <ElFormItem :label="$t('sync_settings.s3.secret_access_key')">
            <ElInput type="password" v-model="s3_settings.secret_access_key" />
          </ElFormItem>
        </template>
        <!-- S3 end -->

        <ElFormItem>
          <ElButton @click="save">{{ $t("sync_settings.save_button") }}</ElButton>
          <ElButton @click="abort_change">{{ $t("sync_settings.abort_button") }}</ElButton>
          <ElButton @click="check">{{ $t("sync_settings.test_button") }}</ElButton>
        </ElFormItem>
        <ElFormItem>
          <ElButton type="danger" @click="upload_all">{{ $t("sync_settings.overwrite_upload") }}</ElButton>
          <ElButton type="danger" @click="download_all">{{ $t("sync_settings.overwrite_download") }}</ElButton>
        </ElFormItem>
      </ElForm>
    </ElCard>
  </div>
</template>

<style scoped>
.hint {
  margin-left: 10px;
  color: #808080;
}

.bold {
  margin-left: 10px;
  font-weight: bold;
  color: var(--el-text-color-primary);
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

.el-select {
  width: 300px;
}
</style>
