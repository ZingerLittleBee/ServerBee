import {ConfigService} from "@nestjs/config";
import {Injectable} from "@nestjs/common";
import {kClientAccessExpireTime} from "@/utils/JwtUtil";

@Injectable()
export class JwtUtilService {
    private defaultClientAccessExpireTime = 3600 * 24 * 30

    constructor(private readonly configService: ConfigService,) {}

    getClientAccessExpireTime() {
        if (!this.configService.get(kClientAccessExpireTime)) {
            return this.defaultClientAccessExpireTime
        }
        const expireTime = parseInt(this.configService.get(kClientAccessExpireTime)!)
        return isNaN(expireTime) ? this.defaultClientAccessExpireTime : expireTime
    }
}
