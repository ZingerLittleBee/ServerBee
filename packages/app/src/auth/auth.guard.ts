import {
    CanActivate,
    ExecutionContext,
    Injectable,
    UnauthorizedException
} from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { Request } from 'express'
import { JwtUtilService } from '@/utils/jwt.util.service'

@Injectable()
export class AuthGuard implements CanActivate {
    constructor(
        private jwtService: JwtService,
        private jwtUtilService: JwtUtilService
    ) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const token = this.extractTokenFromHeader(request)
        if (!token) {
            throw new UnauthorizedException()
        }
        try {
            const payload = await this.jwtService.verifyAsync<{
                userId: string
                iat: number
                exp: number
            }>(token, {
                secret: this.jwtUtilService.getUserAccessSecret()
            })
            request['userId'] = payload.userId
        } catch (e) {
            throw new UnauthorizedException(e.message)
        }
        return true
    }

    private extractTokenFromHeader(request: Request): string | undefined {
        return request.cookies['access_token']
    }
}
