export const SERVER_DETAIL_TABS = ['metrics', 'traffic', 'security', 'ip-quality'] as const
export type ServerDetailTab = (typeof SERVER_DETAIL_TABS)[number]
