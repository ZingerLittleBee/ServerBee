import {
    CanActivate,
    ExecutionContext,
    Injectable,
    Logger
} from '@nestjs/common'
import { lookup } from 'geoip-lite'
import { DeviceType } from '@server-octopus/shared'
import { UserDevice } from '@server-octopus/types'
import { UAParser } from 'ua-parser-js'

@Injectable()
export class UserDeviceGuard implements CanActivate {
    private logger = new Logger(UserDeviceGuard.name)

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()

        const ip = request.ip
        this.logger.verbose(`IP: ${ip}`)
        const geo = lookup(ip)
        let location = ''
        if (geo) {
            location = `${geo.country} ${geo.region} ${geo.city}`
        }

        const ua: string = request.headers['user-agent']
        this.logger.verbose(`User-Agent: ${ua}`)
        const application = request.headers['application']

        const uaResult = new UAParser(ua).getResult()

        let deviceType: DeviceType
        if (ua.toUpperCase().includes(DeviceType.CLIENT)) {
            deviceType = DeviceType.CLIENT
        } else {
            switch (uaResult.device.type) {
                case 'wearable':
                case 'mobile':
                    deviceType = DeviceType.MOBILE
                    break
                case 'tablet':
                    deviceType = DeviceType.TABLET
                    break
                case 'console':
                case 'smarttv':
                case 'embedded':
                    deviceType = DeviceType.DESKTOP
                    break
                default:
                    deviceType = DeviceType.UNKNOWN
            }
        }
        request['ud'] = {
            location,
            ip,
            name: `${uaResult.browser.name} ${uaResult.browser.version}`,
            os: `${uaResult.os.name} ${uaResult.os.version}`,
            type: deviceType,
            app: application
        } as UserDevice
        return true
    }
}
