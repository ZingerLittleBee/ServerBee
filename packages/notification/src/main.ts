import { NestFactory } from '@nestjs/core'
import { ConfigService } from '@nestjs/config'
import { kNatsServer } from '@server-octopus/shared'
import { NotificationModule } from './notification.module'
import { MicroserviceOptions, Transport } from '@nestjs/microservices'

async function bootstrap() {
    const app = await NestFactory.create(NotificationModule)
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
