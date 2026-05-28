const ns = globalThis.__SERVERBEE_SDK__
if (!ns) {
  throw new Error('widget-sdk shim: host did not mount __SERVERBEE_SDK__')
}
export const defineWidget = ns.defineWidget
export const z = ns.z
export const createActionsHelper = ns.createActionsHelper
export const renderConfigForm = ns.renderConfigForm
export const useServers = ns.useServers
export const useServer = ns.useServer
export const useMetric = ns.useMetric
export const useCapability = ns.useCapability
export const useApiQuery = ns.useApiQuery
export const useApiMutation = ns.useApiMutation
export const useAlerts = ns.useAlerts
export const useServiceMonitors = ns.useServiceMonitors
export const useTraffic = ns.useTraffic
export const useUptime = ns.useUptime
export const useHistory = ns.useHistory
export const useGeoIp = ns.useGeoIp
export const useTheme = ns.useTheme
export const useConfigUpdate = ns.useConfigUpdate
export const SDK_VERSION = ns.SDK_VERSION
