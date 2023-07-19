import {
    CanActivate,
    ExecutionContext,
    Injectable,
    Logger
} from '@nestjs/common'
import DeviceDetector from 'node-device-detector'
import DeviceHelper from 'node-device-detector/helper'
import { lookup } from 'geoip-lite'
import { DeviceType } from '@server-octopus/shared'
import { UserDevice } from '@server-octopus/types'

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

        const detector = new DeviceDetector({
            clientIndexes: true,
            deviceIndexes: true,
            deviceAliasCode: false
        })
        const ua: string = request.headers['user-agent']
        this.logger.verbose(`User-Agent: ${ua}`)
        const application = request.headers['application']
        const result = detector.detect(ua)
        let deviceType = DeviceType.UNKNOWN
        if (ua.toUpperCase().includes(DeviceType.CLIENT)) {
            deviceType = DeviceType.CLIENT
        } else if (DeviceHelper.isMobile(result)) {
            deviceType = DeviceType.MOBILE
        } else if (DeviceHelper.isTablet(result)) {
            deviceType = DeviceType.TABLET
        } else if (DeviceHelper.isDesktop(result)) {
            deviceType = DeviceType.DESKTOP
        }
        request['ud'] = {
            location,
            ip,
            name: result.client.version
                ? `${result.client.name} ${result.client.version}`
                : result.client.name,
            os: result.os.version
                ? `${result.os.name} ${result.os.version}`
                : result.os.name,
            type: deviceType,
            app: application
        } as UserDevice
        return true
    }
}
