import { NestFactory } from '@nestjs/core'
import { AuthModule } from './auth.module'
import { MicroserviceOptions, Transport } from '@nestjs/microservices'
import { ConfigService } from '@nestjs/config'
import { kNatsServer } from '@server-octopus/shared'

async function bootstrap() {
    const app = await NestFactory.create(AuthModule)
    const configService = app.get(ConfigService)
    const servers = configService.get(kNatsServer)

    const microservice = app.connectMicroservice<MicroserviceOptions>({
        transport: Transport.NATS,
        options: {
            servers
        }
    })
    await microservice.listen()
}
bootstrap()
