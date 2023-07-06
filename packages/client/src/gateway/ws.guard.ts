import { CanActivate, ExecutionContext, Injectable } from '@nestjs/common'
import { inspect } from 'util'

@Injectable()
export class WsAuthGuard implements CanActivate {
    async canActivate(context: ExecutionContext): Promise<boolean> {
        const client = context.switchToWs().getClient()

        console.log(inspect(client.data))

        console.log('ws auth')

        return true
    }
}
