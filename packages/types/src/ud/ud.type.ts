import { Result } from '../result.type'

enum DeviceType {
    UNKNOWN = 'UNKNOWN',
    DESKTOP = 'DESKTOP',
    TABLET = 'TABLET',
    MOBILE = 'MOBILE',
    CLIENT = 'CLIENT'
}

type CreateUdParam = {
    userId: string
} & UserDevice

type CreateUdResult = Result<string>

type UserDevice = {
    name: string
    os?: string
    app?: string
    ip?: string
    location?: string
    type?: DeviceType
}

export {
    CreateUdParam,
    UserDevice,
    CreateUdResult
}
