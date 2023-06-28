import { ConfigService } from '@nestjs/config'
import {
    Injectable,
    NotFoundException,
    UnauthorizedException
} from '@nestjs/common'
import {
    kClientAccessExpireTime,
    kClientAccessSecret,
    kUserAccessSecret
} from '@/utils/jwt.const'

@Injectable()
export class JwtUtilService {
    private defaultClientAccessExpireTime = 3600 * 24 * 30

    constructor(private readonly configService: ConfigService) {}

    getUserAccessSecret() {
        const userAccessSecret = this.configService.get(kUserAccessSecret)
        if (userAccessSecret) {
            return userAccessSecret
        } else {
            throw new NotFoundException('user access secret Not Found')
        }
    }

    getClientAccessSecret() {
        const clientAccessSecret = this.configService.get(kClientAccessSecret)
        if (clientAccessSecret) {
            return clientAccessSecret
        } else {
            throw new NotFoundException('client access secret Not Found')
        }
    }

    getClientAccessExpireTime() {
        if (!this.configService.get(kClientAccessExpireTime)) {
            return this.defaultClientAccessExpireTime
        }
        const expireTime = parseInt(
            this.configService.get(kClientAccessExpireTime)!
        )
        return isNaN(expireTime)
            ? this.defaultClientAccessExpireTime
            : expireTime
    }

    /**
     * @description check if the token is expired
     * @param {string} exp
     * @return {boolean} true if expired
     */
    static expireChecker(exp: number): void {
        if (Date.now() >= exp * 1000) {
            throw new UnauthorizedException(`token expired`)
        }
    }
}
