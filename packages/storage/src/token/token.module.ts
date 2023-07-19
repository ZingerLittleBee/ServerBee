import { Module } from '@nestjs/common'
import { TokenService } from './token.service'
import { TokenController } from './token.controller'
import { DbModule } from '@/db/db.module'

@Module({
    imports: [DbModule],
    controllers: [TokenController],
    providers: [TokenService]
})
export class TokenModule {}
