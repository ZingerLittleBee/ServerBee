import { NestFactory } from '@nestjs/core'
import { ConfigService } from '@nestjs/config'
import {
    defaultNotificationServicePort,
    defaultNotificationServiceServerHost,
    kNotificationServicePort,
    kNotificationServiceServerHost
} from '@server-octopus/shared'
import { NotificationModule } from './notification.module'
import { MicroserviceOptions, Transport } from '@nestjs/microservices'

async function bootstrap() {
    const app = await NestFactory.create(NotificationModule)
    const configService = app.get(ConfigService)
    const host =
        configService.get<string>(kNotificationServiceServerHost) ??
        defaultNotificationServiceServerHost
    const port =
        configService.get<number>(kNotificationServicePort) ??
        defaultNotificationServicePort

    const microservice = app.connectMicroservice<MicroserviceOptions>({
        transport: Transport.TCP,
        options: {
            host,
            port
        }
    })
    await microservice.listen()
}
bootstrap()
