enum DeviceType {
    UNKNOWN = 'UNKNOWN',
    DESKTOP = 'DESKTOP',
    LAPTOP = 'LAPTOP',
    WEB = 'WEB',
    MOBILE = 'MOBILE',
    SERVER = 'SERVER'
}

type CreateUdDto = {
    userId: string
    name:string
    app?: string
    ip?: string
    location?: string
    version?: string
    type?: DeviceType
}

export {
    CreateUdDto
}
