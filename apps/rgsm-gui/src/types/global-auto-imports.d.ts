export {};

declare global {
  const computed: (typeof import('vue'))['computed'];
  const inject: (typeof import('vue'))['inject'];
  const nextTick: (typeof import('vue'))['nextTick'];
  const onBeforeUnmount: (typeof import('vue'))['onBeforeUnmount'];
  const onMounted: (typeof import('vue'))['onMounted'];
  const onUnmounted: (typeof import('vue'))['onUnmounted'];
  const reactive: (typeof import('vue'))['reactive'];
  const ref: (typeof import('vue'))['ref'];
  const shallowRef: (typeof import('vue'))['shallowRef'];
  const toRaw: (typeof import('vue'))['toRaw'];
  const watch: (typeof import('vue'))['watch'];
  const watchEffect: (typeof import('vue'))['watchEffect'];

  type Ref<T = unknown> = import('vue').Ref<T>;

  const useRoute: (typeof import('vue-router'))['useRoute'];
  const useRouter: (typeof import('vue-router'))['useRouter'];

  const useDebounceFn: (typeof import('@vueuse/core'))['useDebounceFn'];
  const useDark: (typeof import('@vueuse/core'))['useDark'];

  const $t: (typeof import('../i18n'))['$t'];
  const navigateTo: (typeof import('../router'))['navigateTo'];

  const addActivity: (typeof import('../composables/useActivityCenter'))['addActivity'];
  const updateActivity: (typeof import('../composables/useActivityCenter'))['updateActivity'];
  const dismissActivity: (typeof import('../composables/useActivityCenter'))['dismissActivity'];
  const dismissAll: (typeof import('../composables/useActivityCenter'))['dismissAll'];
  const notifySuccess: (typeof import('../composables/useActivityCenter'))['notifySuccess'];
  const notifyError: (typeof import('../composables/useActivityCenter'))['notifyError'];
  const notifyWarning: (typeof import('../composables/useActivityCenter'))['notifyWarning'];
  const notifyInfo: (typeof import('../composables/useActivityCenter'))['notifyInfo'];
  const useActivityCenter: (typeof import('../composables/useActivityCenter'))['useActivityCenter'];

  const useApplyConfirmation: (typeof import('../composables/useApplyConfirmation'))['useApplyConfirmation'];
  const useCloudSyncStatus: (typeof import('../composables/useCloudSyncStatus'))['useCloudSyncStatus'];
  const useConfig: (typeof import('../composables/useConfig'))['useConfig'];
  const useFeedback: (typeof import('../composables/useFeedback'))['useFeedback'];
  const useGlobalLoading: (typeof import('../composables/useGlobalLoading'))['useGlobalLoading'];
  const useIpcNotificationCollector: (typeof import('../composables/useIpcNotificationCollector'))['useIpcNotificationCollector'];
  const useNavigationLinks: (typeof import('../composables/useNavigationLinks'))['useNavigationLinks'];
  const useSaveListExpandBehavior: (typeof import('../composables/useSaveListExpandBehavior'))['useSaveListExpandBehavior'];
  const useSaveListSort: (typeof import('../composables/useSaveListSort'))['useSaveListSort'];
  const useSidebarResize: (typeof import('../composables/useSidebarResize'))['useSidebarResize'];

  const automationMatchesGame: (typeof import('../composables/useGameAutomation'))['automationMatchesGame'];
  const findGameAutomation: (typeof import('../composables/useGameAutomation'))['findGameAutomation'];
  const isAutoSaveConfigured: (typeof import('../composables/useGameAutomation'))['isAutoSaveConfigured'];
  const getGameManagementPath: (typeof import('../composables/useGameManagementRoute'))['getGameManagementPath'];
  const getGameNameFromRouteParam: (typeof import('../composables/useGameManagementRoute'))['getGameNameFromRouteParam'];
}
