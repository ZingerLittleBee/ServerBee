import { NestFactory } from '@nestjs/core'
import { StorageModule } from './storage.module'
import { MicroserviceOptions, Transport } from '@nestjs/microservices'
import { ConfigService } from '@nestjs/config'
import { defaultNatsServerUrl, kNatsServerUrl } from '@server-octopus/shared'

async function bootstrap() {
    const app = await NestFactory.create(StorageModule)

    const microservice = app.connectMicroservice<MicroserviceOptions>({
        transport: Transport.NATS,
        options: {
            servers:
                app.get(ConfigService).get<string>(kNatsServerUrl) ??
                defaultNatsServerUrl
        }
    })
    await microservice.listen()
}
bootstrap()
